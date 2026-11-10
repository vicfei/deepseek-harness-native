//! Approval/policy layer v0: official presets, a fail-closed gate between
//! tool/call and execution, and the three session policy events every
//! recorded official session opens with (fixture-pinned order and shapes:
//! permission/preset -> sandbox/mode -> approval/policy, before the turn).
//!
//! v0 enforcement is process-level (cwd confinement for bash); kernel-level
//! sandboxing is a later, ledgered slice.

use serde_json::{json, Value};

use crate::session::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPreset {
    /// bash confined to the session workspace; reads allowed (official default).
    WorkspaceWrite,
    /// every gated tool asks; headless one-shot has no answerer, so gated
    /// tools fail closed ("without an available answerer, the request fails
    /// closed" — the official runtime-context wording).
    Ask,
    /// everything allowed (official "danger-full-access" posture).
    DangerFullAccess,
}

impl ApprovalPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalPreset::WorkspaceWrite => "workspace-write",
            ApprovalPreset::Ask => "ask",
            ApprovalPreset::DangerFullAccess => "danger-full-access",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "workspace-write" => Some(ApprovalPreset::WorkspaceWrite),
            "ask" => Some(ApprovalPreset::Ask),
            "danger-full-access" => Some(ApprovalPreset::DangerFullAccess),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    /// Allowed, but the execution must run confined to the workspace cwd.
    AllowConfinedToWorkspace,
    /// Fail closed; the text becomes the tool/result the model sees.
    Deny(String),
}

/// v0 gate. `tool` is the registered tool name; `arguments` is the raw JSON
/// string. Read-style tools (skill, mcp__*) are ungated; bash is the gated
/// surface; unknown tools fall back to the preset's posture.
pub fn gate(preset: ApprovalPreset, tool: &str, arguments: &str) -> Decision {
    let gated = tool == "bash";
    if !gated {
        return Decision::Allow;
    }
    let command = command_of(arguments);
    match preset {
        ApprovalPreset::DangerFullAccess => Decision::Allow,
        ApprovalPreset::Ask => Decision::Deny(
            "denied by approval policy `ask`: no answerer is available in a one-shot \
             run, so the request fails closed. Re-run with --approval \
             workspace-write or danger-full-access if this operation is intended."
                .into(),
        ),
        ApprovalPreset::WorkspaceWrite => {
            // v0 heuristic (ledgered): deny obvious absolute escapes; the
            // real confinement is running with current_dir = workspace.
            if escapes_workspace(&command) {
                Decision::Deny(
                    "denied by approval policy `workspace-write`: the command escapes \
                     the session workspace. Run it from inside the workspace."
                        .into(),
                )
            } else {
                Decision::AllowConfinedToWorkspace
            }
        }
    }
}

fn command_of(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|v| v.get("command").cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Detect `cd <absolute-or-parent-path>` prefixes — the minimal v0 escape
/// heuristic; deep shell analysis is explicitly out of scope (ledger).
fn escapes_workspace(command: &str) -> bool {
    for part in command.split([';', '\n', '&']) {
        let part = part.trim();
        let tok: Vec<&str> = part.split_whitespace().collect();
        if tok.len() >= 2 && tok[0] == "cd" {
            let dest = tok[1];
            if dest.starts_with('/') || dest == ".." || dest.starts_with("../") {
                return true;
            }
        }
    }
    false
}

/// The fixture-pinned policy prefix: three events before the turn starts.
pub fn policy_events(
    preset: ApprovalPreset,
    now: &mut dyn FnMut() -> i64,
    seq: &mut i64,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut push = |kind: &str, data: Value, events: &mut Vec<Event>| {
        *seq += 1;
        events.push(Event {
            kind: kind.into(),
            seq: *seq,
            time: now(),
            data,
        });
    };
    let preset_str = preset.as_str().to_string();
    push(
        "permission/preset",
        json!({ "preset": preset_str }),
        &mut events,
    );
    push("sandbox/mode", json!({ "mode": preset_str }), &mut events);
    push(
        "approval/policy",
        json!({ "policy": if preset == ApprovalPreset::WorkspaceWrite { "ask" } else { preset.as_str() } }),
        &mut events,
    );
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_fails_closed_full_access_allows() {
        assert!(matches!(
            gate(ApprovalPreset::Ask, "bash", r#"{"command":"ls"}"#),
            Decision::Deny(ref t) if t.contains("fails closed")
        ));
        assert_eq!(
            gate(
                ApprovalPreset::DangerFullAccess,
                "bash",
                r#"{"command":"ls"}"#
            ),
            Decision::Allow
        );
    }

    #[test]
    fn workspace_write_confines_and_denies_escapes() {
        assert_eq!(
            gate(
                ApprovalPreset::WorkspaceWrite,
                "bash",
                r#"{"command":"cat /etc/hostname"}"#
            ),
            Decision::AllowConfinedToWorkspace
        );
        assert!(matches!(
            gate(
                ApprovalPreset::WorkspaceWrite,
                "bash",
                r#"{"command":"cd / && rm -x"}"#
            ),
            Decision::Deny(_)
        ));
        assert!(matches!(
            gate(
                ApprovalPreset::WorkspaceWrite,
                "bash",
                r#"{"command":"cd ..; ls"}"#
            ),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn ungated_tools_always_allowed() {
        for preset in [ApprovalPreset::WorkspaceWrite, ApprovalPreset::Ask] {
            assert_eq!(gate(preset, "skill", r#"{"name":"x"}"#), Decision::Allow);
            assert_eq!(
                gate(preset, "mcp__vibe__analyze_options", "{}"),
                Decision::Allow
            );
        }
    }

    #[test]
    fn policy_events_match_fixture_order_and_shapes() {
        let mut t = 1794224647270i64;
        let mut seq = -1i64;
        let events = policy_events(
            ApprovalPreset::WorkspaceWrite,
            &mut || {
                t += 1;
                t
            },
            &mut seq,
        );
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["permission/preset", "sandbox/mode", "approval/policy"]
        );
        assert_eq!(
            events[0].data,
            serde_json::json!({ "preset": "workspace-write" })
        );
        assert_eq!(
            events[1].data,
            serde_json::json!({ "mode": "workspace-write" })
        );
        assert_eq!(events[2].data, serde_json::json!({ "policy": "ask" }));
    }
}
