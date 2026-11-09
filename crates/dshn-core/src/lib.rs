//! dshn-core — native harness kernel, slice 1: the durable session.
//!
//! Target format: the official `session.v4.jsonl(.zstd)` under
//! `DSH_HOME/sessions/<cwd-bucket>/<session-id>/`. Layout, verified against
//! two recorded official sessions (see `tests/fixtures/`):
//!
//! ```json
//! {"type":"session","version":4,"id":"session-…","createdAt":1794224647270,"cwd":"/…","isSeeded":false,"delegationDepth":0}
//! {"type":"<kind>","seq":0,"time":1794224647276,"data":{…}}
//! ```
//!
//! First line is the header; every following line is an event envelope with a
//! monotonically increasing `seq` and non-decreasing `time`. The event
//! vocabulary is open (`data` stays `Value`): unknown kinds are preserved,
//! never dropped — forward-compatible with upstream releases, per SPEC's
//! "compatible by default" posture.

mod session;
pub mod sse;

pub use session::{Event, EventKind, Header, SessionLog, SessionSummary};
pub use sse::{assemble, decode_deepseek_stream, DeepSeekChunk, SseDecoder, SseError, SseItem};
