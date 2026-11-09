//! Sidecar event stream — the v0 IPC contract.
//!
//! The Node sidecar runs the official headless bundle with `--json`, which
//! projects the agent's durable session events as NDJSON on stdout (one JSON
//! object per line; every projected event is a commit point on the durable
//! log). The kernel reads the same lines, so what the kernel sees is exactly
//! what the session log contains — no lossy reformatting at this boundary.
//!
//! Shapes verified against `@deepseek-ai/dsh-headless@0.2.1-rc.5` (P0 spike).

use serde::{Deserialize, Serialize};

/// One NDJSON line from the sidecar's `--json` run projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SidecarEvent {
    /// Run starts; identifies the durable session.
    Session {
        #[serde(rename = "sessionId")]
        session_id: String,
        cwd: String,
    },
    /// Turn/step lifecycle and token usage. Payload kept open: phases gain
    /// fields across upstream releases and v0 only needs `step_end.usage`.
    Status {
        phase: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<serde_json::Value>,
    },
    /// Reasoning trace delta (may be empty).
    Thinking { text: String },
    /// Assistant text delta.
    Text { text: String },
    /// Terminal outcome of the one-shot run — the answer.
    Final { text: String },
}

/// Token usage as reported on `step_end`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

impl SidecarEvent {
    /// Parse one stdout line. Unknown event types (upstream may add kinds)
    /// degrade to `None` — the kernel ignores what it does not model, the
    /// same forward-compat posture as the sidecar's own `ignorable` events.
    pub fn parse_line(line: &str) -> Option<SidecarEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_spike_observed_shape() {
        let cases = [
            (
                r#"{"type":"session","sessionId":"session-293f","cwd":"/tmp/x"}"#,
                SidecarEvent::Session {
                    session_id: "session-293f".into(),
                    cwd: "/tmp/x".into(),
                },
            ),
            (
                r#"{"type":"status","phase":"turn_start","turn":1}"#,
                SidecarEvent::Status {
                    phase: "turn_start".into(),
                    turn: Some(1),
                    step: None,
                    usage: None,
                    reason: None,
                },
            ),
            (
                r#"{"type":"thinking","text":""}"#,
                SidecarEvent::Thinking {
                    text: String::new(),
                },
            ),
            (
                r#"{"type":"text","text":"JSON-OK"}"#,
                SidecarEvent::Text {
                    text: "JSON-OK".into(),
                },
            ),
            (
                r#"{"type":"final","text":"JSON-OK"}"#,
                SidecarEvent::Final {
                    text: "JSON-OK".into(),
                },
            ),
        ];
        for (line, want) in cases {
            assert_eq!(SidecarEvent::parse_line(line), Some(want), "line: {line}");
        }
    }

    #[test]
    fn parses_step_end_usage() {
        let line = r#"{"type":"status","phase":"step_end","turn":1,"step":1,"usage":{"inputTokens":523,"outputTokens":4,"cacheReadTokens":5376,"cacheWriteTokens":0,"totalTokens":5903}}"#;
        match SidecarEvent::parse_line(line) {
            Some(SidecarEvent::Status { usage: Some(u), .. }) => {
                assert_eq!(u.input_tokens, 523);
                assert_eq!(u.total_tokens, 5903);
            }
            other => panic!("bad parse: {other:?}"),
        }
    }

    #[test]
    fn ignores_unknown_and_garbage_lines() {
        assert!(SidecarEvent::parse_line("").is_none());
        assert!(SidecarEvent::parse_line("not json").is_none());
        // unknown event kind: dropped, not fatal
        assert!(SidecarEvent::parse_line(r#"{"type":"future_thing","x":1}"#).is_none());
    }
}
