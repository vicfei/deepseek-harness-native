//! DeepSeek provider edge: one streaming chat completion over HTTPS,
//! feeding the incremental SSE decoder. The kernel stays engine-free —
//! this is plain HTTP + our decoder, no JS anywhere.

use crate::sse::{DeepSeekChunk, SseDecoder, SseItem};
use serde_json::{json, Value};

#[derive(Debug)]
pub enum ProviderError {
    Http(String),
    Stream(&'static str),
    Api { status: u16, body: String },
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Http(m) => write!(f, "http: {m}"),
            ProviderError::Stream(m) => write!(f, "stream: {m}"),
            ProviderError::Api { status, body } => {
                write!(
                    f,
                    "api {status}: {}",
                    body.chars().take(200).collect::<String>()
                )
            }
        }
    }
}
impl std::error::Error for ProviderError {}

pub struct DeepSeekProvider {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        DeepSeekProvider {
            api_key: api_key.into(),
            base_url: "https://api.deepseek.com".into(),
            model: model.into(),
        }
    }

    /// The wire request body for one streamed completion (shape pinned by
    /// the recorded official request/header fixtures and the API's docs).
    pub fn request_body(&self, user_text: &str, max_tokens: u64) -> Value {
        json!({
            "model": self.model,
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_tokens": max_tokens,
            "messages": [{ "role": "user", "content": user_text }]
        })
    }

    /// Stream one completion; every decoded chunk is passed to `on_chunk`
    /// live, and all chunks are returned once the stream completes.
    pub fn stream_chat(
        &self,
        user_text: &str,
        max_tokens: u64,
        mut on_chunk: impl FnMut(&DeepSeekChunk),
    ) -> Result<Vec<DeepSeekChunk>, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let resp = ureq::post(url.as_str())
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(self.request_body(user_text, max_tokens))
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if resp.status() != 200 {
            let status = resp.status();
            let body = resp.into_string().unwrap_or_default();
            return Err(ProviderError::Api { status, body });
        }

        let mut reader = resp.into_reader();
        let mut decoder = SseDecoder::default();
        let mut chunks = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| ProviderError::Http(e.to_string()))?;
            if n == 0 {
                break;
            }
            for item in decoder
                .push(&buf[..n])
                .map_err(|e| ProviderError::Stream(e.0))?
            {
                if let SseItem::Data(data) = item {
                    if data == "[DONE]" {
                        return Ok(chunks);
                    }
                    if let Ok(chunk) = serde_json::from_str::<DeepSeekChunk>(&data) {
                        on_chunk(&chunk);
                        chunks.push(chunk);
                    }
                }
            }
        }
        Ok(chunks)
    }
}
