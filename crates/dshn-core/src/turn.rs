//! Multi-step turn: steps loop on tool calls until a tool-free response,
//! emitting the recorded official sequence (assistant w/ tool-call blocks →
//! tool/call → tool/result → next step; request/header once per turn).

use serde_json::{json, Value};

use crate::session::Event;
use crate::sse::{assemble, assemble_tool_calls, DeepSeekChunk};
use crate::tools::{Registry, ToolOutcome};
use crate::TurnInput;

pub const MAX_STEPS: usize = 50;

/// Stream one step's chunks for the current wire history (provider edge).
pub type StreamFn<'a> = dyn FnMut(&[Value]) -> Result<Vec<DeepSeekChunk>, String> + 'a;

pub fn run_turn(
    input: &TurnInput,
    registry: &Registry,
    stream: &mut StreamFn,
    now: &mut dyn FnMut() -> i64,
    seq: &mut i64,
) -> Result<Vec<Event>, String> {
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

    // wire history: system-less v0; user task first
    let mut messages: Vec<Value> = vec![json!({ "role": "user", "content": input.task })];

    push("turn/start", json!({ "turn": 1 }), &mut events);

    for step in 1..=MAX_STEPS {
        push(
            "step/start",
            json!({ "turn": 1, "step": step }),
            &mut events,
        );
        if step == 1 {
            push(
                "user/message",
                json!({
                    "content": [{ "type": "text", "text": input.task }],
                    "source": { "kind": "user" }, "role": "user"
                }),
                &mut events,
            );
            push(
                "request/header",
                json!({
                    "header": { "config": {
                        "provider": input.provider, "model": input.model,
                        "maxTokens": input.max_tokens, "reasoningEffort": input.reasoning_effort
                    }, "tools": registry.wire_tools() }
                }),
                &mut events,
            );
            push(
                "request/context",
                json!({ "provider": input.provider, "model": input.model }),
                &mut events,
            );
        }

        let chunks = stream(&messages)?;
        let (text, reasoning) = assemble(&chunks);
        let calls = assemble_tool_calls(&chunks);
        let usage = chunks
            .iter()
            .rev()
            .find_map(|c| c.usage.clone())
            .unwrap_or_default();

        let mut content: Vec<Value> = Vec::new();
        if !reasoning.is_empty() {
            content.push(json!({ "type": "reasoning", "text": reasoning }));
        }
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
        for c in &calls {
            content.push(json!({ "type": "tool-call", "id": c.id, "name": c.name, "arguments": c.arguments }));
        }
        push(
            "assistant/message",
            json!({
                "turn": 1, "step": step,
                "message": { "role": "assistant", "content": content,
                    "source": { "kind": "model", "provider": input.provider, "model": input.model } },
                "usage": usage
            }),
            &mut events,
        );

        // append assistant wire message (with tool_calls when present)
        let tool_wire: Vec<Value> = calls
            .iter()
            .map(|c| {
                json!({ "id": c.id, "type": "function",
                "function": { "name": c.name, "arguments": c.arguments } })
            })
            .collect();
        let assistant = if tool_wire.is_empty() {
            json!({ "role": "assistant", "content": text })
        } else {
            json!({ "role": "assistant", "content": text, "tool_calls": tool_wire })
        };
        messages.push(assistant);

        if calls.is_empty() {
            push("step/end", json!({ "turn": 1, "step": step }), &mut events);
            push(
                "turn/end",
                json!({ "turn": 1, "reason": { "kind": "completed" } }),
                &mut events,
            );
            return Ok(events);
        }

        // execute each call in order; emit call -> result pairs
        for call in &calls {
            push(
                "tool/call",
                json!({
                    "turn": 1, "step": step, "callId": call.id,
                    "name": call.name, "arguments": call.arguments
                }),
                &mut events,
            );
            let outcome: ToolOutcome = match registry.find(&call.name) {
                Some(tool) => tool.execute(&call.arguments),
                None => ToolOutcome {
                    text: format!("unknown tool {}", call.name),
                    is_error: true,
                },
            };
            push(
                "tool/result",
                json!({
                    "turn": 1, "step": step,
                    "message": { "role": "tool",
                        "source": { "kind": "tool", "callId": call.id },
                        "toolCallId": call.id,
                        "content": [{ "type": "text", "text": outcome.text }] }
                }),
                &mut events,
            );
            messages
                .push(json!({ "role": "tool", "tool_call_id": call.id, "content": outcome.text }));
        }
        push("step/end", json!({ "turn": 1, "step": step }), &mut events);
    }
    Err(format!("turn exceeded {MAX_STEPS} steps"))
}
