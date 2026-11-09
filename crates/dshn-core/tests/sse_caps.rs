//! Per-event byte caps (issue #3) — the three regression anchors:
//! line cap enforced DURING accumulation, per-event budget that resets at
//! event boundaries, and the stream guard demoted to a runaway guard.

use dshn_core::sse::{MAX_EVENT_BYTES, MAX_LINE_BYTES};
use dshn_core::{SseDecoder, SseItem};

#[test]
fn over_long_line_rejected_during_accumulation() {
    // MAX_LINE_BYTES + 1 bytes with NO newline: rejected immediately —
    // the buffer never holds more than the cap, no 4 MiB feeding required.
    let mut d = SseDecoder::default();
    let chunk = vec![b'a'; MAX_LINE_BYTES + 1];
    let err = d.push(&chunk).expect_err("must reject during accumulation");
    assert!(format!("{err:?}").contains("line too long"), "got: {err:?}");
}

#[test]
fn long_run_of_small_events_decodes_past_4mib() {
    // Defect 3 regression: >4 MiB of many small well-formed events is
    // legitimate traffic and must decode (the budget is per-event).
    let mut d = SseDecoder::default();
    let event = b"data: tick\n\n";
    let mut fed = 0usize;
    while fed <= 4 << 20 {
        let out = d.push(event).expect("legitimate long stream must decode");
        assert_eq!(out, vec![SseItem::Data("tick".to_string())]);
        fed += event.len();
    }
}

#[test]
fn single_oversized_event_hits_event_cap_not_stream_cap() {
    // One joined event whose accumulated data: lines exceed MAX_EVENT_BYTES:
    // the error names the event cap, not the stream guard.
    let mut d = SseDecoder::default();
    let line = format!("data: {}\n", "a".repeat(MAX_LINE_BYTES / 3 + 1));
    d.push(line.as_bytes()).unwrap();
    d.push(line.as_bytes()).unwrap();
    let err = d.push(line.as_bytes()).expect_err("third line crosses MAX_EVENT_BYTES");
    assert!(format!("{err:?}").contains("event too large"), "got: {err:?}");
    assert!(MAX_EVENT_BYTES < 4 << 20, "anchor sanity: event cap below the old stream kill line");
}
