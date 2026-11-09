//! SSE oracle tests against a REAL recorded DeepSeek stream (deepseek-flash,
//! streamed, captured verbatim — 53 data events incl. reasoning + usage).

use dshn_core::{assemble, decode_deepseek_stream, SseDecoder, SseItem};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/deepseek-stream.sse"
    ))
    .unwrap()
}

#[test]
fn decodes_recorded_stream_and_reassembles_the_answer() {
    let raw = fixture();
    let chunks = decode_deepseek_stream(&raw).unwrap();
    assert!(
        chunks.len() >= 50,
        "recorded stream carries {} chunks",
        chunks.len()
    );
    let (text, reasoning) = assemble(&chunks);
    assert_eq!(
        text.trim(),
        "1 2 3 4 5",
        "reassembled answer must match the prompt"
    );
    assert!(
        !reasoning.is_empty(),
        "deepseek-flash streams reasoning_content"
    );
    assert!(
        chunks.iter().any(|c| c.usage.is_some()),
        "usage rides the final chunks"
    );
    assert!(chunks.iter().any(|c| c
        .choices
        .iter()
        .any(|ch| ch.finish_reason.as_deref() == Some("stop"))));
}

#[test]
fn boundary_independence_one_byte_at_a_time() {
    let raw = fixture();
    let whole = decode_deepseek_stream(&raw).unwrap();
    let mut dec = SseDecoder::default();
    let mut byte_split = Vec::new();
    for b in &raw {
        for item in dec.push(std::slice::from_ref(b)).unwrap() {
            if let SseItem::Data(d) = item {
                byte_split.push(d);
            }
        }
    }
    let whole_data: Vec<String> = decode_data_only(&raw);
    assert_eq!(
        byte_split, whole_data,
        "1-byte chunking must yield identical events"
    );
    assert_eq!(
        whole.len() + 1,
        byte_split.len(),
        "…including the [DONE] terminator"
    );
}

#[test]
fn boundary_independence_random_splits() {
    let raw = fixture();
    let mut seed = 0x9e37u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..20 {
        let mut dec = SseDecoder::default();
        let mut got = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let n = 1 + (next() as usize % 997);
            let end = (i + n).min(raw.len());
            for item in dec.push(&raw[i..end]).unwrap() {
                if let SseItem::Data(d) = item {
                    got.push(d);
                }
            }
            i = end;
        }
        assert_eq!(
            got,
            decode_data_only(&raw),
            "random split must yield identical events"
        );
    }
}

#[test]
fn crlf_and_comments_and_multiline_join() {
    // synthetic edge cases the recorded stream may not contain
    let mut dec = SseDecoder::default();
    let items = dec
        .push(b":keepalive\r\ndata: a\r\ndata: b\r\n\r\ndata: c\n\n")
        .unwrap();
    assert_eq!(
        items,
        vec![
            SseItem::Comment,
            SseItem::Data("a\nb".into()),
            SseItem::Data("c".into())
        ]
    );
}

fn decode_data_only(raw: &[u8]) -> Vec<String> {
    let mut dec = SseDecoder::default();
    let mut out = Vec::new();
    for item in dec.push(raw).unwrap() {
        if let SseItem::Data(d) = item {
            out.push(d);
        }
    }
    out
}
