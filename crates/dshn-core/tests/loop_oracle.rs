//! Oracle: the native loop must emit the SAME core event sequence the
//! recorded official no-tool session shows, with the observed data shapes.

use dshn_core::run_one_shot;
use dshn_core::session::SessionLog;
use dshn_core::sse::decode_deepseek_stream;
use dshn_core::TurnInput;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
}

/// Kinds both sides own (upstream interleaves plugin-added kinds; the native
/// v0 kernel emits exactly the core protocol).
const CORE: &[&str] = &[
    "turn/start",
    "step/start",
    "user/message",
    "request/header",
    "request/context",
    "assistant/message",
    "step/end",
    "turn/end",
];

/// Core kinds with component-injected user/messages (source.kind !=
/// "user": runtime-context snapshots, skill-catalog reminders) excluded —
/// those belong to their native components (later slices), and the official
/// fixture pins exactly which seams they are.
fn core_kinds(events: &[dshn_core::session::Event]) -> Vec<String> {
    events
        .iter()
        .filter(|e| CORE.contains(&e.kind.as_str()))
        .filter(|e| {
            e.kind != "user/message"
                || e.data
                    .pointer("/source/kind")
                    .map(|v| v == "user")
                    .unwrap_or(true)
        })
        .map(|e| e.kind.clone())
        .collect()
}

#[test]
fn native_sequence_matches_the_recorded_official_turn() {
    let official = SessionLog::parse(&fixture("session-v4-trivial.jsonl")).unwrap();
    let chunks = decode_deepseek_stream(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/deepseek-stream.sse"
        ))
        .unwrap(),
    )
    .unwrap();

    let input = TurnInput {
        task: "Count from 1 to 5, digits only, space separated.".into(),
        provider: "deepseek-official".into(),
        model: "deepseek-flash".into(),
        max_tokens: 256000,
        reasoning_effort: "high".into(),
    };
    let mut t = 1794224647270i64;
    let native = run_one_shot(
        &input,
        &chunks,
        &mut || {
            t += 1;
            t
        },
        &mut (-1),
    );

    // 0) the official fixture pins the injection seams for later slices
    let injected: Vec<&str> = official
        .events
        .iter()
        .filter(|e| e.kind == "user/message")
        .filter(|e| e.data.pointer("/source/kind").and_then(|v| v.as_str()) != Some("user"))
        .filter_map(|e| e.data.pointer("/source/kind").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(injected, vec!["runtime-context", "skill-catalog"]);

    // 1) exact core kind sequence, in order
    assert_eq!(core_kinds(&native), core_kinds(&official.events));

    // 2) the assistant message mirrors the observed shape: reasoning block +
    //    text block, reassembled from the stream
    let am = native
        .iter()
        .find(|e| e.kind == "assistant/message")
        .unwrap();
    let content = am.data.pointer("/message/content").unwrap();
    let blocks: Vec<&str> = content
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.get("type").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(blocks, vec!["reasoning", "text"]);
    let text = content.as_array().unwrap()[1]
        .get("text")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(text.trim(), "1 2 3 4 5");
    assert!(am.data.get("usage").is_some());

    // 3) turn/end carries the observed completed reason
    let te = native.iter().find(|e| e.kind == "turn/end").unwrap();
    assert_eq!(te.data.pointer("/reason/kind").unwrap(), "completed");

    // 4) the produced events form a valid session log (invariants + final)
    let jsonl = {
        let mut out = String::from(
            "{\"type\":\"session\",\"version\":4,\"id\":\"session-native\",\"createdAt\":1,\"cwd\":\"/\"}\n",
        );
        for e in &native {
            out.push_str(&serde_json::to_string(e).unwrap());
            out.push('\n');
        }
        out
    };
    let log = SessionLog::parse(&jsonl).unwrap();
    log.check_invariants().unwrap();
    assert_eq!(
        log.final_text().as_deref().map(str::trim),
        Some("1 2 3 4 5")
    );
    assert_eq!(log.summarize().turns, 1);
    assert_eq!(log.summarize().steps, 1);
}
