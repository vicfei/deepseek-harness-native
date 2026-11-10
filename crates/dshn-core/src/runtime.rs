//! Runtime-context injection: the user/message between the task and the
//! skill-catalog reminder that tells the model where it runs and under what
//! policy — shapes pinned by the recorded official fixture.

use serde_json::{json, Value};

/// Build the three sections for one run. Text formats follow the fixture
/// verbatim; only the variable parts (cwd, policy names) are substituted.
pub fn sections(cwd: &str, preset: crate::policy::ApprovalPreset) -> Vec<Value> {
    let (sandbox_text, approval_text) = match preset {
        crate::policy::ApprovalPreset::WorkspaceWrite => (
            format!(
                "Current DSH file policy: workspace-write. Any available operation enforced by the DSH file sandbox may modify files under the session workspace: \"{cwd}\". Some platform temporary areas may also be writable."
            ),
            "Approval policy: ask. Operations that require approval may ask through the configured answerers; without an available answerer, the request fails closed.".to_string(),
        ),
        crate::policy::ApprovalPreset::Ask => (
            format!("Current DSH file policy: ask. Operations that modify files require approval; the session workspace is \"{cwd}\"."),
            "Approval policy: ask. Operations that require approval may ask through the configured answerers; without an available answerer, the request fails closed.".to_string(),
        ),
        crate::policy::ApprovalPreset::DangerFullAccess => (
            "Current DSH file policy: danger-full-access. Operations may modify files anywhere on this host; no sandbox is in force.".to_string(),
            "Approval policy: danger-full-access. Operations run without approval prompts.".to_string(),
        ),
    };
    vec![
        json!({
            "name": "working-directory:current",
            "text": format!("Current working directory: \"{cwd}\".")
        }),
        json!({ "name": "sandbox:policy", "text": sandbox_text }),
        json!({ "name": "approval:policy", "text": approval_text }),
    ]
}

/// The full user/message event data: source.kind=runtime-context with the
/// sections array, body assembled exactly like the fixture (header line,
/// then each section text separated by blank lines).
pub fn message_data(cwd: &str, preset: crate::policy::ApprovalPreset) -> Value {
    let secs = sections(cwd, preset);
    let mut body = String::from(
        "Current runtime context. This snapshot supersedes earlier runtime-context snapshots.",
    );
    for s in &secs {
        body.push_str("\n\n");
        body.push_str(s["text"].as_str().unwrap_or(""));
    }
    json!({
        "content": [{ "type": "text", "text": body }],
        "source": { "kind": "runtime-context", "form": "snapshot", "sections": secs },
        "role": "user"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ApprovalPreset;

    #[test]
    fn workspace_write_matches_fixture_shapes() {
        let data = message_data("/home/ubuntu/dshx-spike", ApprovalPreset::WorkspaceWrite);
        assert_eq!(
            data.pointer("/source/kind"),
            Some(&json!("runtime-context"))
        );
        assert_eq!(data.pointer("/source/form"), Some(&json!("snapshot")));
        let names: Vec<&str> = data["source"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "working-directory:current",
                "sandbox:policy",
                "approval:policy"
            ]
        );
        let body = data["content"][0]["text"].as_str().unwrap();
        assert!(body.starts_with(
            "Current runtime context. This snapshot supersedes earlier runtime-context snapshots."
        ));
        assert!(body.contains("Current working directory: \"/home/ubuntu/dshx-spike\"."));
        assert!(body.contains("Approval policy: ask."));
        assert!(body.contains("fails closed"));
    }

    #[test]
    fn presets_change_their_sections() {
        let full = message_data("/w", ApprovalPreset::DangerFullAccess);
        let body = full["content"][0]["text"].as_str().unwrap();
        assert!(body.contains("danger-full-access"));
        assert!(!body.contains("workspace-write"));
    }
}
