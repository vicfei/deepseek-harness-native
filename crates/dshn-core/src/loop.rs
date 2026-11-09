//! Native agent loop v0: one turn, one step, no tools — the exact core
//! sequence the recorded official sessions show (plugin-added kinds such as
//! session/title* and session-log-deepseek/* arrive when those components
//! exist natively; see SPEC's ledger).
//!
//! Pure core: chunks come in already decoded (from the SSE decoder), a clock
//! closure mints times — fully deterministic under tests, wired to real HTTP
//! by the provider edge (next slice).

use serde_json::{json, Value};

use crate::session::Event;
use crate::sse::{assemble, DeepSeekChunk};

#[derive(Debug, Clone)]
pub struct TurnInput {
    pub task: String,
    pub provider: String,
    pub model: String,
    pub max_tokens: u64,
    pub reasoning_effort: String,
}

/// Run one no-tool turn, emitting the faithful core event sequence.
/// `chunks` is the provider stream for this step; `now` mints epoch millis;
/// `seq` is the next event seq (caller owns sequencing).
pub fn run_one_shot(
    input: &TurnInput,
    chunks: &[DeepSeekChunk],
    now: &mut dyn FnMut() -> i64,
    seq: &mut i64,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut push = |kind: &str, data: Value, events: &mut Vec<Event>| {
        *seq += 1;
        events.push(Event {
            kind: kind.to_string(),
            seq: *seq,
            time: now(),
            data,
        });
    };

    push("turn/start", json!({ "turn": 1 }), &mut events);
    push("step/start", json!({ "turn": 1, "step": 1 }), &mut events);
    push(
        "user/message",
        json!({
            "content": [{ "type": "text", "text": input.task }],
            "source": { "kind": "user" },
            "role": "user",
            "id": uuid_placeholder(),
        }),
        &mut events,
    );
    push(
        "request/header",
        json!({
            "header": {
                "config": {
                    "provider": input.provider,
                    "model": input.model,
                    "maxTokens": input.max_tokens,
                    "reasoningEffort": input.reasoning_effort,
                },
                "tools": []
            }
        }),
        &mut events,
    );
    push(
        "request/context",
        json!({
            "provider": input.provider,
            "model": input.model,
        }),
        &mut events,
    );

    let (text, reasoning) = assemble(chunks);
    let usage = chunks
        .iter()
        .rev()
        .find_map(|c| c.usage.clone())
        .unwrap_or_default();
    let mut content = Vec::new();
    if !reasoning.is_empty() {
        content.push(json!({ "type": "reasoning", "text": reasoning }));
    }
    content.push(json!({ "type": "text", "text": text }));
    push(
        "assistant/message",
        json!({
            "turn": 1,
            "step": 1,
            "message": {
                "role": "assistant",
                "content": content,
                "source": { "kind": "model", "provider": input.provider, "model": input.model }
            },
            "usage": usage
        }),
        &mut events,
    );
    push("step/end", json!({ "turn": 1, "step": 1 }), &mut events);
    push(
        "turn/end",
        json!({ "turn": 1, "reason": { "kind": "completed" } }),
        &mut events,
    );
    events
}

fn uuid_placeholder() -> String {
    // v0: deterministic placeholder; real UUIDs arrive with the runtime slice
    "00000000-0000-4000-8000-000000000000".to_string()
}
