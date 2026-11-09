//! `dshn` — community Rust runtime for the DeepSeek Harness ecosystem.
//!
//! v0 CLI surface:
//!
//!   dshn [--sidecar DIR] [--home DIR] [--node PATH] [--timeout SECS]
//!        [--events] "one-shot task text"
//!   dshn provision [--sidecar DIR] [--registry URL]   # stage + install the pinned sidecar
//!
//! Default layout mirrors the official harness: `$DSHN_HOME` (or `~/.dshn`)
//! holds `sidecar/` (boot.mjs + pinned official bundles) and `home/`
//! (profiles/ + skills/ = DSH_HOME for the run).

mod events;
mod host;
mod provision;

use std::{path::PathBuf, process::ExitCode, time::Duration};

use events::SidecarEvent;
use host::{resolve_paths, JsHost, NodeSidecar, TaskRequest};

const DEFAULT_TIMEOUT_SECS: u64 = 600;
const USAGE: &str =
    "usage: dshn [--sidecar DIR] [--home DIR] [--node PATH] [--timeout SECS] [--events] \"task\"
       dshn provision [--sidecar DIR] [--registry URL]";

struct Cli {
    task: Option<String>,
    sidecar: Option<PathBuf>,
    home: Option<PathBuf>,
    node: PathBuf,
    timeout: Option<u64>,
    events: bool,
    command: Command,
    registry: Option<String>,
}

enum Command {
    Run,
    Provision,
}

fn parse_args(argv: &[String]) -> Result<Cli, String> {
    let mut cli = Cli {
        task: None,
        sidecar: None,
        home: None,
        node: PathBuf::from("node"),
        timeout: Some(DEFAULT_TIMEOUT_SECS),
        events: false,
        command: Command::Run,
        registry: None,
    };
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        let take_value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| format!("missing value after {a}"))
        };
        match a.as_str() {
            "provision" => cli.command = Command::Provision,
            "--sidecar" => cli.sidecar = Some(PathBuf::from(take_value(&mut i)?)),
            "--home" => cli.home = Some(PathBuf::from(take_value(&mut i)?)),
            "--node" => cli.node = PathBuf::from(take_value(&mut i)?),
            "--registry" => cli.registry = Some(take_value(&mut i)?),
            "--timeout" => {
                let v = take_value(&mut i)?;
                cli.timeout = Some(
                    v.parse()
                        .map_err(|_| format!("--timeout: not a number: {v}"))?,
                );
            }
            "--no-timeout" => cli.timeout = None,
            "--events" => cli.events = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            _ if a.starts_with("--") => return Err(format!("unknown flag {a}\n{USAGE}")),
            _ => {
                if cli.task.replace(a.clone()).is_some() {
                    return Err(format!("only one task argument is supported\n{USAGE}"));
                }
            }
        }
        i += 1;
    }
    Ok(cli)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&argv) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };

    let paths = resolve_paths(cli.sidecar.as_deref(), cli.home.as_deref());

    match cli.command {
        Command::Provision => match provision::provision(&paths.sidecar, cli.registry.as_deref()) {
            Ok(dir) => {
                println!("sidecar provisioned at {}", dir.display());
                println!(
                    "next: put profiles/ and skills/ under {}, then: dshn \"task\"",
                    paths.home.display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("dshn: provision failed: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Run => {
            let Some(task) = cli.task else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            let host = NodeSidecar::new(&cli.node, &paths.sidecar, &paths.home);
            let request = TaskRequest {
                text: task,
                timeout: cli.timeout.map(Duration::from_secs),
            };
            let mut sink = EventSink { raw: cli.events };
            match host.run_task(&request, &mut |ev| sink.emit(&ev)) {
                Ok(outcome) => {
                    if !cli.events {
                        eprintln!(
                            "-- session {} · {} events · in {} out {} tokens · exit {:?}",
                            outcome.session_id.as_deref().unwrap_or("-"),
                            outcome.events_seen,
                            outcome.usage.input_tokens,
                            outcome.usage.output_tokens,
                            outcome.exit_code
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("dshn: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// Human mode streams assistant text deltas as they arrive and stays quiet
/// otherwise; `--events` forwards the raw NDJSON untouched (for pipelines).
struct EventSink {
    raw: bool,
}

impl EventSink {
    /// Human mode: live turn/step progress on stderr, the final answer alone
    /// on stdout (text events are per-step payloads; printing them live would
    /// duplicate the final). Raw mode: every event as NDJSON on stdout.
    fn emit(&mut self, event: &SidecarEvent) {
        match event {
            SidecarEvent::Status {
                phase, turn, step, ..
            } if !self.raw => {
                eprintln!(
                    ".. {phase} (turn {}/{})",
                    turn.unwrap_or(0),
                    step.unwrap_or(0)
                );
            }
            SidecarEvent::Final { text } if !self.raw => println!("{text}"),
            SidecarEvent::Final { .. } => {}
            _ if self.raw => println!("{}", serde_json::to_string(event).unwrap_or_default()),
            _ => {}
        }
    }
}
