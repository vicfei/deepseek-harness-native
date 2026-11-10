//! Oracle tests against recorded official sessions (DSH 0.2.1-rc.5, captured
//! from the P0/P1 sidecar runs). The native session core must read the real
//! artifacts before it may claim compatibility.

use dshn_core::SessionLog;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).expect("fixture present")
}

#[test]
fn parses_recorded_trivial_session_and_invariants_hold() {
    let log = SessionLog::parse(&fixture("session-v4-trivial.jsonl")).unwrap();
    assert!(log.header.id.starts_with("session-"));
    assert_eq!(log.header.version, 4);
    log.check_invariants()
        .expect("seq monotonic, time non-decreasing");
    let summary = log.summarize();
    assert_eq!(summary.turns, 1);
    // observed truth: 1 real prompt + 2 inbox-spliced protocol messages
    assert_eq!(
        summary.user_messages, 3,
        "user/message = prompt + internal splices"
    );
    assert!(
        summary.event_count >= 15,
        "trivial run still carries full protocol events"
    );
    assert_eq!(
        log.final_text().as_deref(),
        Some("DSHN-M1-OK"),
        "final == last text block"
    );
}

#[test]
fn parses_recorded_tool_run_session() {
    let log = SessionLog::parse(&fixture("session-v4-toolrun.jsonl")).unwrap();
    log.check_invariants().unwrap();
    let summary = log.summarize();
    // the recorded run is the M1 trustquant smoke: multi-step tool use
    assert_eq!(
        summary.steps, 9,
        "recorded M1 tool run: 9 steps (oracle-pinned)"
    );
    assert_eq!(summary.assistant_texts.len(), 9);
    let kinds: Vec<String> = log.events.iter().map(|e| e.kind.clone()).collect();
    assert!(
        kinds.iter().any(|k| k.contains("tool")),
        "tool events present: {:?}",
        kinds.iter().filter(|k| k.contains("tool")).count()
    );
    let final_text = log.final_text().expect("assistant answered");
    assert!(
        final_text.contains("trustquant")
            || final_text.contains("replay")
            || final_text.len() > 100,
        "final text should be the smoke answer"
    );
}

#[test]
fn round_trips_unknown_kinds_verbatim() {
    let raw = fixture("session-v4-trivial.jsonl");
    let log = SessionLog::parse(&raw).unwrap();
    // every original line parses; unknown kinds survive with their data
    let original = raw.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(log.events.len() + 1, original);
    let other: Vec<String> = log
        .events
        .iter()
        .filter_map(|e| match e.classify() {
            dshn_core::EventKind::Other(k) => Some(k),
            _ => None,
        })
        .collect();
    assert!(!other.is_empty(), "open vocabulary preserved: {other:?}");
}

#[test]
fn rejects_wrong_version_and_garbage() {
    assert!(SessionLog::parse("").is_err());
    assert!(SessionLog::parse("not json").is_err());
    assert!(SessionLog::parse(
        "{\"type\":\"session\",\"version\":3,\"id\":\"x\",\"createdAt\":1,\"cwd\":\"/\"}"
    )
    .is_err());
}

#[test]
fn issue_5_zstd_container_round_trip_magic_and_atomicity() {
    let log = SessionLog::parse(&fixture("session-v4-trivial.jsonl")).unwrap();
    let dir = std::env::temp_dir().join("dshn-zstd-writer-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session-native-xyz/session.v4.jsonl.zstd");

    log.write_container(&path).unwrap();

    // 1) round-trip through the decode side
    let bytes = std::fs::read(&path).unwrap();
    let back = SessionLog::parse_container(&bytes, true).unwrap();
    assert_eq!(back.header, log.header);
    assert_eq!(back.events, log.events);
    assert_eq!(back.final_text(), log.final_text());

    // 2) zstd frame magic
    assert_eq!(&bytes[..4], &[0x28, 0xB5, 0x2F, 0xFD], "zstd magic");

    // 3) atomicity: no tmp leftovers anywhere in the tree
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .chain(std::fs::read_dir(path.parent().unwrap()).unwrap())
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "tmp leftovers: {leftovers:?}");
}
