//! Incremental SSE framing + DeepSeek chunk translation (oracle: the
//! recorded stream in `tests/fixtures/deepseek-stream.sse`).
//!
//! Decoder contract mirrors the upstream harness provider tests: output is
//! independent of how the network splits chunks — the same events fall out
//! whether the bytes arrive whole, per-line, or one byte at a time.

use serde::{Deserialize, Serialize};

/// One decoded SSE item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseItem {
    Data(String),
    Comment,
}

/// Hard caps (defensive; the recorded stream is ~16 KB).
pub const MAX_LINE_BYTES: usize = 1 << 20;
pub const MAX_EVENT_BYTES: usize = 1 << 20;

#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
    data: Vec<String>,
    has_data: bool,
    total: usize,
}

#[derive(Debug)]
pub struct SseError(pub &'static str);

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseItem>, SseError> {
        let mut out = Vec::new();
        for &b in bytes {
            self.total += 1;
            if self.total > MAX_EVENT_BYTES * 4 {
                return Err(SseError("stream too large"));
            }
            match b {
                b'\n' => {
                    if self.buf.last() == Some(&b'\r') {
                        self.buf.pop();
                    }
                    let line = std::mem::take(&mut self.buf);
                    if line.len() > MAX_LINE_BYTES {
                        return Err(SseError("line too long"));
                    }
                    self.dispatch(&line, &mut out);
                }
                _ => self.buf.push(b),
            }
        }
        Ok(out)
    }

    fn dispatch(&mut self, line: &[u8], out: &mut Vec<SseItem>) {
        if line.is_empty() {
            // blank line = event boundary; emit accumulated data joined
            if self.has_data {
                out.push(SseItem::Data(self.data.join("\n")));
                self.data.clear();
                self.has_data = false;
            }
            return;
        }
        if line.starts_with(b":") {
            out.push(SseItem::Comment);
            return;
        }
        let (field, value) = match line.iter().position(|&b| b == b':') {
            Some(i) => {
                let v = &line[i + 1..];
                let v = if v.first() == Some(&b' ') { &v[1..] } else { v };
                (&line[..i], v)
            }
            None => (line, &line[..0]),
        };
        if field == b"data" {
            self.data.push(String::from_utf8_lossy(value).into_owned());
            self.has_data = true;
        }
        // other fields (event:, id:, retry:) are ignored — not used by
        // DeepSeek's chat stream; kept deliberately out of v0.
    }
}

/// One DeepSeek chat-completion stream chunk (`data: {...}` payload).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DeepSeekChunk {
    #[serde(default)]
    pub id: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Choice {
    pub delta: Delta,
    #[serde(default, rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, rename = "reasoning_content")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
}

/// One streamed tool-call fragment; fields arrive incrementally and the
/// arguments string accumulates across fragments of the same index.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ToolCallDelta {
    #[serde(default)]
    pub index: u64,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<FunctionDelta>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct FunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// A fully-assembled tool call (after merging stream fragments by index).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Merge tool-call fragments from a chunk sequence, in index order.
pub fn assemble_tool_calls(chunks: &[DeepSeekChunk]) -> Vec<ToolCall> {
    let mut slots: Vec<(u64, String, String, String)> = Vec::new(); // (index,id,name,args)
    for c in chunks {
        for ch in &c.choices {
            for tc in &ch.delta.tool_calls {
                let idx = tc.index as usize;
                while slots.len() <= idx {
                    slots.push((idx as u64, String::new(), String::new(), String::new()));
                }
                let slot = &mut slots[idx];
                if let Some(id) = &tc.id {
                    if !id.is_empty() {
                        slot.1 = id.clone();
                    }
                }
                if let Some(f) = &tc.function {
                    if let Some(n) = &f.name {
                        if !n.is_empty() {
                            slot.2 = n.clone();
                        }
                    }
                    if let Some(a) = &f.arguments {
                        slot.3.push_str(a);
                    }
                }
            }
        }
    }
    slots
        .into_iter()
        .map(|(_, id, name, args)| ToolCall {
            id,
            name,
            arguments: args,
        })
        .collect()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Usage {
    #[serde(default, rename = "prompt_tokens")]
    pub prompt_tokens: u64,
    #[serde(default, rename = "completion_tokens")]
    pub completion_tokens: u64,
    #[serde(default, rename = "prompt_cache_hit_tokens")]
    pub prompt_cache_hit_tokens: u64,
    #[serde(default, rename = "prompt_cache_miss_tokens")]
    pub prompt_cache_miss_tokens: u64,
}

/// Decode a full recorded stream into DeepSeek chunks; `[DONE]` terminates.
pub fn decode_deepseek_stream(raw: &[u8]) -> Result<Vec<DeepSeekChunk>, SseError> {
    let mut dec = SseDecoder::default();
    let mut chunks = Vec::new();
    for item in dec.push(raw)? {
        if let SseItem::Data(data) = item {
            if data == "[DONE]" {
                break;
            }
            if let Ok(chunk) = serde_json::from_str::<DeepSeekChunk>(&data) {
                chunks.push(chunk);
            }
        }
    }
    Ok(chunks)
}

/// Reassemble the assistant's text + reasoning from a chunk sequence.
pub fn assemble(chunks: &[DeepSeekChunk]) -> (String, String) {
    let mut text = String::new();
    let mut reasoning = String::new();
    for c in chunks {
        for ch in &c.choices {
            if let Some(d) = &ch.delta.content {
                text.push_str(d);
            }
            if let Some(r) = &ch.delta.reasoning_content {
                reasoning.push_str(r);
            }
        }
    }
    (text, reasoning)
}
