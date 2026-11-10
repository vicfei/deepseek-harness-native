//! The durable session: header + append-only event envelopes + projection.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// First line of a v4 session file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Header {
    #[serde(rename = "type")]
    pub kind: String, // "session"
    pub version: u32,
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    pub cwd: String,
    #[serde(rename = "isSeeded", default)]
    pub is_seeded: bool,
    #[serde(rename = "delegationDepth", default)]
    pub delegation_depth: u32,
}

/// One event envelope. `kind` is open vocabulary; `data` is opaque here —
/// typed views live with the components that own them (P3 slices).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    /// Envelope fields are REQUIRED (issue #6): every oracle line carries
    /// both; a line missing them must fail HERE, not break invariants later.
    pub seq: i64,
    pub time: i64,
    #[serde(default)]
    pub data: Value,
}

/// Typed views over the observed core kinds (everything else stays `Event`).
#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Session,
    UserMessage { text: String },
    AssistantMessage { text: String },
    TurnStart,
    TurnEnd,
    StepStart,
    StepEnd,
    RequestHeader,
    Other(String),
}

impl Event {
    pub fn classify(&self) -> EventKind {
        match self.kind.as_str() {
            "session" => EventKind::Session,
            "user/message" => EventKind::UserMessage {
                text: text_of(self.data.get("content")),
            },
            "assistant/message" => EventKind::AssistantMessage {
                text: text_of(self.data.pointer("/message/content")),
            },
            "turn/start" => EventKind::TurnStart,
            "turn/end" => EventKind::TurnEnd,
            "step/start" => EventKind::StepStart,
            "step/end" => EventKind::StepEnd,
            "request/header" => EventKind::RequestHeader,
            other => EventKind::Other(other.to_string()),
        }
    }
}

/// Extract text from an observed `content` shape: either a plain string or a
/// block list `[{type:"text"|"reasoning", text}, …]` (text blocks only).
fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[derive(Debug, Clone)]
pub struct SessionLog {
    pub header: Header,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub turns: u32,
    pub steps: u32,
    pub user_messages: u32,
    pub assistant_texts: Vec<String>,
    pub title: Option<String>,
    pub duration_ms: i64,
    pub event_count: usize,
}

impl SessionLog {
    /// Parse one session from decompressed JSONL bytes (header first).
    /// Unknown event kinds are preserved verbatim.
    pub fn parse(jsonl: &str) -> Result<Self, String> {
        let mut lines = jsonl.lines().filter(|l| !l.trim().is_empty());
        let header: Header = serde_json::from_str(lines.next().ok_or("empty session")?)
            .map_err(|e| format!("header: {e}"))?;
        if header.version != 4 {
            return Err(format!("unsupported session version {}", header.version));
        }
        let mut events = Vec::new();
        for (i, line) in lines.enumerate() {
            let event: Event = serde_json::from_str(line)
                .map_err(|e| format!("event line {} (1-based, header = line 1): {e}", i + 2))?;
            events.push(event);
        }
        Ok(SessionLog { header, events })
    }

    /// Parse from the on-disk artifact, transparently handling the `.zstd`
    /// container the official persistence plugin writes.
    pub fn parse_container(bytes: &[u8], zstd: bool) -> Result<Self, String> {
        let jsonl = if zstd {
            let mut out = Vec::new();
            let mut reader = ruzstd::StreamingDecoder::new(std::io::Cursor::new(bytes))
                .map_err(|e| format!("zstd: {e}"))?;
            std::io::Read::read_to_end(&mut reader, &mut out)
                .map_err(|e| format!("zstd read: {e}"))?;
            String::from_utf8(out).map_err(|e| format!("utf8: {e}"))?
        } else {
            std::str::from_utf8(bytes)
                .map_err(|e| format!("utf8: {e}"))?
                .to_string()
        };
        Self::parse(&jsonl)
    }

    /// Write the official durable container: header + events as JSONL,
    /// zstd-compressed, atomic (tmp + rename) — the counterpart of
    /// `parse_container(bytes, true)`.
    pub fn write_container(&self, path: &std::path::Path) -> Result<(), String> {
        let mut jsonl = serde_json::to_string(&self.header).map_err(|e| format!("header: {e}"))?;
        jsonl.push('\n');
        for e in &self.events {
            jsonl.push_str(&serde_json::to_string(e).map_err(|err| format!("event: {err}"))?);
            jsonl.push('\n');
        }
        let compressed =
            zstd::encode_all(jsonl.as_bytes(), 3).map_err(|e| format!("zstd encode: {e}"))?;
        let tmp = path.with_extension("jsonl.zstd.tmp");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        std::fs::write(&tmp, &compressed).map_err(|e| format!("write tmp: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("rename: {e}")
        })
    }

    /// Envelope invariants from the observed format: seq strictly increasing,
    /// time non-decreasing (header's createdAt precedes all events).
    pub fn check_invariants(&self) -> Result<(), String> {
        let mut last_seq = -1i64;
        let mut last_time = self.header.created_at;
        for (i, e) in self.events.iter().enumerate() {
            if e.seq <= last_seq {
                return Err(format!(
                    "event {i} ({}): seq {} not > {}",
                    e.kind, e.seq, last_seq
                ));
            }
            if e.time < last_time {
                return Err(format!(
                    "event {i} ({}): time {} < {}",
                    e.kind, e.time, last_time
                ));
            }
            last_seq = e.seq;
            last_time = e.time;
        }
        Ok(())
    }

    /// Fold events into the summary a headless consumer needs.
    pub fn summarize(&self) -> SessionSummary {
        let mut s = SessionSummary {
            id: self.header.id.clone(),
            event_count: self.events.len(),
            ..Default::default()
        };
        let mut first_time = None;
        let mut last_time = self.header.created_at;
        for e in &self.events {
            match e.classify() {
                EventKind::TurnStart => s.turns += 1,
                EventKind::StepStart => s.steps += 1,
                EventKind::UserMessage { .. } => s.user_messages += 1,
                EventKind::AssistantMessage { text } => s.assistant_texts.push(text),
                EventKind::Other(k) if k == "session/title" => {
                    s.title = e
                        .data
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                _ => {}
            }
            if first_time.is_none() {
                first_time = Some(e.time);
            }
            last_time = e.time;
        }
        let _ = first_time;
        s.duration_ms = last_time - self.header.created_at;
        s
    }

    /// The final assistant text — what the headless projection calls `final`.
    pub fn final_text(&self) -> Option<String> {
        self.events.iter().rev().find_map(|e| {
            if e.kind != "assistant/message" {
                return None;
            }
            let joined = text_of(e.data.pointer("/message/content"));
            (!joined.is_empty()).then_some(joined)
        })
    }
}
