//! JsHost — the seam between the pure-Rust kernel and the JS ecosystem lane.
//!
//! v0 ships exactly one implementation: [`NodeSidecar`], which spawns the
//! sidecar bootstrap (`boot.mjs`) under a real Node ≥22 and speaks the NDJSON
//! event contract in [`crate::events`]. The trait exists so later hosts
//! (deno-embedded, …) can slot in without touching callers — the engine choice
//! is a deployment detail; per SPEC.md the kernel itself is engine-free.

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crate::events::{SidecarEvent, Usage};

#[derive(Debug)]
pub struct TaskRequest {
    pub text: String,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Default)]
pub struct RunOutcome {
    pub session_id: Option<String>,
    pub final_text: Option<String>,
    pub usage: Usage,
    pub events_seen: usize,
    pub exit_code: Option<i32>,
}

#[derive(Debug)]
pub enum HostError {
    /// The sidecar produced no `final` event before exiting.
    NoFinal {
        exit_code: Option<i32>,
        events_seen: usize,
    },
    /// The run exceeded its deadline and the child was killed.
    TimedOut { after: Duration },
    /// The sidecar could not be started at all.
    Spawn(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::NoFinal {
                exit_code,
                events_seen,
            } => write!(
                f,
                "sidecar ended without a final event (exit={:?}, {} events seen)",
                exit_code, events_seen
            ),
            HostError::TimedOut { after } => {
                write!(f, "sidecar timed out after {:.0}s", after.as_secs_f32())
            }
            HostError::Spawn(msg) => write!(f, "sidecar failed to start: {msg}"),
        }
    }
}
impl std::error::Error for HostError {}

pub trait JsHost {
    /// Run one one-shot task to completion. `on_event` receives every parsed
    /// sidecar event in order (live), including the terminal `Final`.
    fn run_task(
        &self,
        request: &TaskRequest,
        on_event: &mut dyn FnMut(SidecarEvent),
    ) -> Result<RunOutcome, HostError>;
}

/// Spawns `node <sidecar>/boot.mjs --json <task>` with `DSH_HOME` bound to the
/// given harness home. Stderr is inherited (provider/diagnostic noise stays
/// visible); stdout is the event stream.
#[derive(Debug, Clone)]
pub struct NodeSidecar {
    pub node: PathBuf,
    pub sidecar_dir: PathBuf,
    pub home: PathBuf,
}

impl NodeSidecar {
    pub fn new(
        node: impl Into<PathBuf>,
        sidecar_dir: impl Into<PathBuf>,
        home: impl Into<PathBuf>,
    ) -> Self {
        NodeSidecar {
            node: node.into(),
            sidecar_dir: sidecar_dir.into(),
            home: home.into(),
        }
    }

    /// The official bundles use Node 22+ APIs (`fs.globSync`, zstd in
    /// `node:zlib`, `Promise.withResolvers`, `module.stripTypeScriptTypes`).
    /// Under an older Node the tree fails mid-boot with six confusing import
    /// errors — so the kernel gates up front, naming the fix.
    fn require_node_22(&self) -> Result<(), HostError> {
        let out = Command::new(&self.node)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| HostError::Spawn(format!("{}: {e}", self.node.display())))?;
        let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let major = version
            .strip_prefix('v')
            .unwrap_or(&version)
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok());
        match major {
            Some(m) if m >= 22 => Ok(()),
            _ => Err(HostError::Spawn(format!(
                "node >= 22 required, {} reports '{}' — point --node at a Node 22+ binary (e.g. via nvm)",
                self.node.display(),
                if version.is_empty() { "?" } else { &version }
            ))),
        }
    }
}

/// The two filesystem locations a run needs: the provisioned sidecar
/// (boot.mjs + node_modules) and the DSH_HOME (profiles/ + skills/).
#[derive(Debug, Clone)]
pub struct Paths {
    pub sidecar: PathBuf,
    pub home: PathBuf,
}

/// Explicit flags win; otherwise both live under `$DSHN_HOME` (default
/// `~/.dshn`), mirroring how the official harness keeps `~/.dsh` separate.
pub fn resolve_paths(sidecar: Option<&Path>, home: Option<&Path>) -> Paths {
    let root = std::env::var_os("DSHN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let user_home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
            Path::new(&user_home).join(".dshn")
        });
    Paths {
        sidecar: sidecar
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.join("sidecar")),
        home: home
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.join("home")),
    }
}

impl JsHost for NodeSidecar {
    fn run_task(
        &self,
        request: &TaskRequest,
        on_event: &mut dyn FnMut(SidecarEvent),
    ) -> Result<RunOutcome, HostError> {
        let boot = self.sidecar_dir.join("boot.mjs");
        if !boot.is_file() {
            return Err(HostError::Spawn(format!(
                "boot script not found at {} (run `dshn provision` or pass --sidecar)",
                boot.display()
            )));
        }
        if !self.home.is_dir() {
            return Err(HostError::Spawn(format!(
                "DSH_HOME not found at {} (expected profiles/ and skills/ inside)",
                self.home.display()
            )));
        }
        self.require_node_22()?;

        let mut child = Command::new(&self.node)
            .arg(&boot)
            .arg("--json")
            .arg(&request.text)
            .env("DSH_HOME", &self.home)
            .current_dir(&self.sidecar_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| HostError::Spawn(format!("{}: {e}", self.node.display())))?;

        // Reader thread parses stdout lines into events and ships them over a
        // channel; the main loop forwards, accumulates, and enforces the
        // deadline (a `&mut dyn FnMut` cannot cross a thread boundary).
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel::<Option<SidecarEvent>>();
        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if tx.send(SidecarEvent::parse_line(&line)).is_err() {
                    break;
                }
            }
        });

        let started = Instant::now();
        let mut outcome = RunOutcome::default();
        let mut timed_out = false;
        loop {
            // Outer None = channel disconnected = stream closed.
            let msg: Option<Option<SidecarEvent>> = match request.timeout {
                Some(t) => match rx.recv_timeout(
                    t.saturating_sub(started.elapsed())
                        .max(Duration::from_millis(1)),
                ) {
                    Ok(m) => Some(m),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        timed_out = true;
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => None,
                },
                None => rx.recv().ok(),
            };
            let Some(payload) = msg else { break };
            // Inner None = an unparseable/unknown stdout line; skip it.
            let Some(event) = payload else { continue };
            outcome.events_seen += 1;
            match &event {
                SidecarEvent::Session { session_id, .. } => {
                    outcome.session_id = Some(session_id.clone())
                }
                SidecarEvent::Status { usage: Some(u), .. } => outcome.usage = u.clone(),
                SidecarEvent::Final { text } => outcome.final_text = Some(text.clone()),
                _ => {}
            }
            on_event(event);
        }

        if timed_out {
            drop(rx);
            let _ = child.kill();
        }
        reader.join().expect("reader thread must not panic");
        outcome.exit_code = child.wait().ok().and_then(|s| s.code());

        if timed_out {
            return Err(HostError::TimedOut {
                after: started.elapsed(),
            });
        }
        if outcome.final_text.is_none() {
            return Err(HostError::NoFinal {
                exit_code: outcome.exit_code,
                events_seen: outcome.events_seen,
            });
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_flags_win_over_default_root() {
        let p = resolve_paths(Some(Path::new("/s")), Some(Path::new("/h")));
        assert_eq!(p.sidecar, Path::new("/s"));
        assert_eq!(p.home, Path::new("/h"));
    }

    #[test]
    fn spawn_error_names_missing_boot() {
        let host = NodeSidecar::new("node", "/definitely/not/here", "/tmp");
        let err = host
            .run_task(
                &TaskRequest {
                    text: "hi".into(),
                    timeout: Some(Duration::from_secs(1)),
                },
                &mut |_| {},
            )
            .unwrap_err();
        assert!(
            matches!(err, HostError::Spawn(ref m) if m.contains("boot script")),
            "{err}"
        );
    }
}
