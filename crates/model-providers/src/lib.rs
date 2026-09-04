//! Model provider layer 2.0 (plan §14, Codex-shaped).
//!
//! - `run_turn` is **async** and **streaming**: token deltas flow through an
//!   mpsc channel while the turn runs, so the UI renders incrementally.
//! - Every call is **cancellable** (CancellationToken); `ProviderError::Cancelled`
//!   is a first-class outcome, not an error string.
//! - OpenAI-compatible providers use SSE (`stream: true`); the parser is a
//!   pure state machine with unit tests.
//! - Connection failures retry with exponential backoff (transport errors,
//!   429, 5xx); a mid-stream stall trips an idle watchdog.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_calls: Vec::new(), tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_calls: Vec::new(), tool_call_id: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_calls: Vec::new(), tool_call_id: None }
    }
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self { temperature: 0.7, top_p: 1.0, max_tokens: 4096 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSchema>,
    pub sampling: SamplingParams,
    pub force_json: bool,
    /// Stream token deltas (requires a streaming-capable provider).
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnResponse {
    pub message: ChatMessage,
    pub finish_reason: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCapabilities {
    pub tool_calls: bool,
    pub json_response: bool,
    pub streaming: bool,
}

/// Incremental events emitted while a turn runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnStreamEvent {
    Delta { text: String },
    ToolCallDelta { id: String, name: String, arguments_delta: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("http: {0}")]
    Http(String),
    #[error("api status {status}: {body}")]
    Api { status: u16, body: String },
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("cancelled")]
    Cancelled,
    #[error("timeout: {0}")]
    Timeout(String),
}

/// Retry decision (pure — unit tested). `attempt` is 0-based.
pub fn should_retry(status: u16, attempt: u32, max_retries: u32) -> bool {
    attempt < max_retries && (status == 429 || status >= 500)
}

fn backoff_delay(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(1000u64.saturating_mul(1u64 << attempt.min(4)))
}

#[async_trait]
pub trait StoryboardModelProvider: Send + Sync {
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    fn capabilities(&self) -> ProviderCapabilities;
    /// Run one model turn. Deltas stream into `events` as they arrive; the
    /// final assembled response is returned. Honours `cancel` at every await.
    async fn run_turn(
        &self,
        req: TurnRequest,
        cancel: CancellationToken,
        events: mpsc::Sender<TurnStreamEvent>,
    ) -> Result<TurnResponse, ProviderError>;
}

// ---------------------------------------------------------------------------
// SSE parsing (pure state machine — unit tested)
// ---------------------------------------------------------------------------

/// Feeds bytes, yields complete SSE `data:` payloads (without the prefix).
pub struct SseParser {
    buf: String,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.push_str(&String::from_utf8_lossy(bytes));
        let mut out = Vec::new();
        // SSE events are separated by a blank line (\n\n or \r\n\r\n).
        loop {
            let sep = self
                .buf
                .find("\n\n")
                .map(|i| (i, 2))
                .or_else(|| self.buf.find("\r\n\r\n").map(|i| (i, 4)));
            let Some((idx, len)) = sep else { break };
            let event: String = self.buf[..idx].to_string();
            self.buf = self.buf[idx + len..].to_string();
            let data: Vec<&str> = event
                .lines()
                .map(|l| l.strip_prefix("data:").map(|d| d.strip_prefix(' ').unwrap_or(d)).unwrap_or(""))
                .filter(|l| !l.is_empty())
                .collect();
            if !data.is_empty() {
                out.push(data.join("\n"));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible provider (streaming)
// ---------------------------------------------------------------------------

pub struct OpenAiCompatibleProvider {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub max_retries: u32,
    /// Idle watchdog between stream chunks.
    pub idle_timeout: std::time::Duration,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(provider_id: &str, base_url: &str, api_key: &str, model: &str) -> Self {
        Self::with_options(provider_id, base_url, api_key, model, 2, 120)
    }

    pub fn with_options(
        provider_id: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        max_retries: u32,
        idle_timeout_secs: u64,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            base_url: base_url.trim_end_matches('/').into(),
            api_key: api_key.into(),
            model_name: model.into(),
            max_retries,
            idle_timeout: std::time::Duration::from_secs(idle_timeout_secs),
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
        }
    }

    fn build_body(&self, req: &TurnRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model_name,
            "messages": req.messages,
            "temperature": req.sampling.temperature,
            "top_p": req.sampling.top_p,
            "max_tokens": req.sampling.max_tokens,
            "stream": req.stream,
        });
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(
                req.tools
                    .iter()
                    .map(|t| serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters_json,
                        }
                    }))
                    .collect::<Vec<_>>()
            );
        }
        if req.force_json {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        body
    }
}

struct ToolCallAccum {
    id: String,
    name: String,
    arguments: String,
}

#[async_trait]
impl StoryboardModelProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }
    fn model(&self) -> &str {
        &self.model_name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities { tool_calls: true, json_response: true, streaming: true }
    }

    async fn run_turn(
        &self,
        req: TurnRequest,
        cancel: CancellationToken,
        events: mpsc::Sender<TurnStreamEvent>,
    ) -> Result<TurnResponse, ProviderError> {
        use futures_util::StreamExt;

        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_body(&req);

        // connect (retry/backoff for pre-stream failures only)
        let mut response = None;
        for attempt in 0..=self.max_retries {
            tokio::select! {
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                res = self.client.post(&url)
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .send() => {
                    match res {
                        Ok(r) if r.status().is_success() => {
                            response = Some(r);
                            break;
                        }
                        Ok(r) => {
                            let status = r.status().as_u16();
                            let text = r.text().await.unwrap_or_default();
                            if !should_retry(status, attempt, self.max_retries) {
                                return Err(ProviderError::Api { status, body: text });
                            }
                        }
                        Err(e) => {
                            if attempt >= self.max_retries {
                                return Err(ProviderError::Http(e.to_string()));
                            }
                        }
                    }
                }
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                _ = tokio::time::sleep(backoff_delay(attempt)) => {}
            }
        }
        let Some(response) = response else {
            return Err(ProviderError::Http("no response".into()));
        };

        // stream + assemble
        let mut parser = SseParser::new();
        let mut content = String::new();
        let mut tool_calls: Vec<ToolCallAccum> = Vec::new();
        let mut finish_reason = "stop".to_string();
        let mut usage = None;
        let mut stream = response.bytes_stream();

        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                c = tokio::time::timeout(self.idle_timeout, stream.next()) => {
                    match c {
                        Err(_) => return Err(ProviderError::Timeout("stream idle watchdog".into())),
                        Ok(v) => v,
                    }
                }
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| ProviderError::Http(e.to_string()))?;
            for payload in parser.feed(&chunk) {
                if payload == "[DONE]" {
                    return Ok(TurnResponse {
                        message: ChatMessage {
                            role: Role::Assistant,
                            content,
                            tool_calls: tool_calls
                                .into_iter()
                                .map(|a| ToolCall { id: a.id, name: a.name, arguments_json: a.arguments })
                                .collect(),
                            tool_call_id: None,
                        },
                        finish_reason,
                        usage,
                    });
                }
                let v: serde_json::Value = serde_json::from_str(&payload)
                    .map_err(|e| ProviderError::Parse(e.to_string()))?;
                if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                    usage = Some(Usage {
                        prompt_tokens: u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                        completion_tokens: u.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                    });
                }
                let choice = v.get("choices").and_then(|c| c.get(0));
                if let Some(fr) = choice.and_then(|c| c.get("finish_reason")).and_then(|f| f.as_str()) {
                    finish_reason = fr.to_string();
                }
                let Some(delta) = choice.and_then(|c| c.get("delta")) else {
                    continue;
                };
                if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        content.push_str(text);
                        let _ = events.send(TurnStreamEvent::Delta { text: text.into() }).await;
                    }
                }
                if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        while tool_calls.len() <= index {
                            tool_calls.push(ToolCallAccum { id: String::new(), name: String::new(), arguments: String::new() });
                        }
                        let acc = &mut tool_calls[index];
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if !id.is_empty() {
                                acc.id = id.into();
                            }
                        }
                        if let Some(f) = tc.get("function") {
                            if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                                if !name.is_empty() {
                                    acc.name = name.into();
                                }
                            }
                            if let Some(args) = f.get("arguments").and_then(|a| a.as_str()) {
                                acc.arguments.push_str(args);
                                let _ = events
                                    .send(TurnStreamEvent::ToolCallDelta {
                                        id: acc.id.clone(),
                                        name: acc.name.clone(),
                                        arguments_delta: args.into(),
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
        }
        Ok(TurnResponse {
            message: ChatMessage {
                role: Role::Assistant,
                content,
                tool_calls: tool_calls
                    .into_iter()
                    .map(|a| ToolCall { id: a.id, name: a.name, arguments_json: a.arguments })
                    .collect(),
                tool_call_id: None,
            },
            finish_reason,
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// Mock provider (streaming + cancellable)
// ---------------------------------------------------------------------------

pub struct MockProvider {
    pub provider_id: String,
    pub model_name: String,
    pub script: Vec<TurnResponse>,
    /// when true the mock never answers — used to exercise cancellation
    pub hang: bool,
    /// delay between emitted delta chunks
    pub chunk_delay: std::time::Duration,
    cursor: AtomicUsize,
}

impl MockProvider {
    pub fn new(script: Vec<TurnResponse>) -> Self {
        Self {
            provider_id: "mock".into(),
            model_name: "mock-model".into(),
            script,
            hang: false,
            chunk_delay: std::time::Duration::from_millis(5),
            cursor: AtomicUsize::new(0),
        }
    }

    pub fn hanging() -> Self {
        Self { hang: true, ..Self::new(Vec::new()) }
    }

    /// Text answer; content is streamed word-by-word (proves delta plumbing).
    pub fn streaming_text(content: &str) -> Self {
        Self::simple_text(content)
    }

    pub fn simple_text(content: &str) -> Self {
        Self::new(vec![TurnResponse {
            message: ChatMessage::assistant(content),
            finish_reason: "stop".into(),
            usage: None,
        }])
    }

    pub fn tool_call(name: &str, arguments_json: &str) -> Self {
        Self::new(vec![TurnResponse {
            message: ChatMessage {
                role: Role::Assistant,
                content: String::new(),
                tool_calls: vec![ToolCall { id: "call_1".into(), name: name.into(), arguments_json: arguments_json.into() }],
                tool_call_id: None,
            },
            finish_reason: "tool_calls".into(),
            usage: None,
        }])
    }

    pub fn tool_then_text(name: &str, arguments_json: &str, final_text: &str) -> Self {
        Self::new(vec![
            TurnResponse {
                message: ChatMessage {
                    role: Role::Assistant,
                    content: String::new(),
                    tool_calls: vec![ToolCall { id: "call_1".into(), name: name.into(), arguments_json: arguments_json.into() }],
                    tool_call_id: None,
                },
                finish_reason: "tool_calls".into(),
                usage: None,
            },
            TurnResponse {
                message: ChatMessage::assistant(final_text),
                finish_reason: "stop".into(),
                usage: None,
            },
        ])
    }
}

#[async_trait]
impl StoryboardModelProvider for MockProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }
    fn model(&self) -> &str {
        &self.model_name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities { tool_calls: true, json_response: true, streaming: true }
    }

    async fn run_turn(
        &self,
        _req: TurnRequest,
        cancel: CancellationToken,
        events: mpsc::Sender<TurnStreamEvent>,
    ) -> Result<TurnResponse, ProviderError> {
        if self.hang {
            // sleep forever unless cancelled — cancellation test fixture
            tokio::select! {
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                _ = tokio::time::sleep(std::time::Duration::from_secs(3600)) => {}
            }
        }
        let i = self.cursor.fetch_add(1, Ordering::SeqCst);
        let r = self
            .script
            .get(i)
            .or_else(|| self.script.last())
            .cloned()
            .ok_or_else(|| ProviderError::Unsupported("empty mock script".into()))?;
        // stream the content in small chunks so deltas are observable
        for piece in r.message.content.split_whitespace() {
            tokio::select! {
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                _ = tokio::time::sleep(self.chunk_delay) => {}
            }
            let _ = events.send(TurnStreamEvent::Delta { text: format!("{piece} ") }).await;
        }
        Ok(r)
    }
}

/// Arc-wrap a provider for the runtime.
pub fn arc(p: impl StoryboardModelProvider + 'static) -> Arc<dyn StoryboardModelProvider> {
    Arc::new(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_handles_split_chunks_and_done() {
        let mut p = SseParser::new();
        // one event split across two TCP chunks
        let mut out = p.feed(b"data: {\"choices\":[{\"delta\":{");
        assert!(out.is_empty());
        out.extend(p.feed(b"\"content\":\"Hi\"}}]}\n\n"));
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("\"content\":\"Hi\""));
        // [DONE] surfaces as its own payload
        let out2 = p.feed(b"data: [DONE]\n\n");
        assert_eq!(out2, vec!["[DONE]".to_string()]);
    }

    #[test]
    fn retry_decision_table() {
        assert!(should_retry(429, 0, 2));
        assert!(should_retry(500, 1, 2));
        assert!(!should_retry(500, 2, 2));
        assert!(!should_retry(429, 0, 0));
        assert!(!should_retry(400, 0, 2));
        assert!(!should_retry(401, 0, 2));
    }

    #[test]
    fn backoff_is_exponential() {
        assert_eq!(backoff_delay(0).as_millis(), 1000);
        assert_eq!(backoff_delay(1).as_millis(), 2000);
        assert_eq!(backoff_delay(9).as_millis(), 16000);
    }

    #[tokio::test]
    async fn mock_streams_deltas_and_can_cancel() {
        let mock = MockProvider::streaming_text("hello streaming world");
        let (tx, mut rx) = mpsc::channel(64);
        let resp = mock
            .run_turn(
                TurnRequest {
                    messages: vec![ChatMessage::user("hi")],
                    tools: vec![],
                    sampling: SamplingParams::default(),
                    force_json: false,
                    stream: true,
                },
                CancellationToken::new(),
                tx,
            )
            .await
            .unwrap();
        assert_eq!(resp.message.content, "hello streaming world");
        let mut deltas = Vec::new();
        while let Some(e) = rx.recv().await {
            if let TurnStreamEvent::Delta { text } = e {
                deltas.push(text);
            }
        }
        assert_eq!(deltas.len(), 3);

        // cancellation aborts a hanging provider
        let hanging = std::sync::Arc::new(MockProvider::hanging());
        let cancel = CancellationToken::new();
        let cancel_cloned = cancel.clone();
        let (tx2, _rx2) = mpsc::channel(4);
        let req = TurnRequest {
            messages: vec![ChatMessage::user("x")],
            tools: vec![],
            sampling: SamplingParams::default(),
            force_json: false,
            stream: false,
        };
        let handle = tokio::spawn(async move { hanging.run_turn(req, cancel.clone(), tx2).await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancel_cloned.cancel();
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, ProviderError::Cancelled));
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = TurnRequest {
            messages: vec![ChatMessage::system("s"), ChatMessage::user("u")],
            tools: vec![ToolSchema {
                name: "search_templates".into(),
                description: "d".into(),
                parameters_json: serde_json::json!({"type": "object"}),
            }],
            sampling: SamplingParams::default(),
            force_json: false,
            stream: true,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: TurnRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tools.len(), 1);
        assert!(back.stream);
    }
}
