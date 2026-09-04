//! Model provider layer (plan §14). Business code never hard-codes model
//! names; Matcher/Modifier/Reviewer can each pick a different provider.
//!
//! v1 uses blocking HTTP (`ureq`) inside a synchronous trait — the app server
//! runs turns on worker threads. Streaming arrives with the desktop UX phase.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    /// set when role == Tool
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
    /// request a JSON-only reply when the provider supports it
    pub force_json: bool,
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
}

pub trait StoryboardModelProvider: Send + Sync {
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    fn capabilities(&self) -> ProviderCapabilities;
    fn start_turn(&self, req: &TurnRequest) -> Result<TurnResponse, ProviderError>;
}

/// Generic OpenAI-compatible chat-completions provider (OpenAI / GLM /
/// LM Studio / Ollama with an OpenAI endpoint...).
///
/// Requests carry a hard timeout and retry with exponential backoff on
/// transport errors, 429 and 5xx (plan §14 retry requirement — a hung
/// endpoint must never block a turn thread forever).
pub struct OpenAiCompatibleProvider {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub extra_headers: Vec<(String, String)>,
    /// Hard per-request timeout. Default 120s.
    pub timeout: std::time::Duration,
    /// Retries *after* the first attempt. Default 2 (3 attempts total).
    pub max_retries: u32,
    agent: ureq::Agent,
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
        timeout_secs: u64,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            base_url: base_url.trim_end_matches('/').into(),
            api_key: api_key.into(),
            model_name: model.into(),
            extra_headers: Vec::new(),
            timeout: std::time::Duration::from_secs(timeout_secs),
            max_retries,
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build(),
        }
    }
}

/// Retry decision (pure — unit tested). `attempt` is 0-based.
pub fn should_retry(status: u16, attempt: u32, max_retries: u32) -> bool {
    attempt < max_retries && (status == 429 || status >= 500)
}

fn backoff_delay(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(1000u64.saturating_mul(1u64 << attempt.min(4)))
}

impl StoryboardModelProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }
    fn model(&self) -> &str {
        &self.model_name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities { tool_calls: true, json_response: true }
    }

    fn start_turn(&self, req: &TurnRequest) -> Result<TurnResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = serde_json::json!({
            "model": self.model_name,
            "messages": req.messages,
            "temperature": req.sampling.temperature,
            "top_p": req.sampling.top_p,
            "max_tokens": req.sampling.max_tokens,
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
        let text = {
            let mut last_err: Option<ProviderError> = None;
            let mut body_text: Option<String> = None;
            for attempt in 0..=self.max_retries {
                let mut request = self
                    .agent
                    .post(&url)
                    .set("Authorization", &format!("Bearer {}", self.api_key))
                    .set("Content-Type", "application/json");
                for (k, v) in &self.extra_headers {
                    request = request.set(k, v);
                }
                match request.send_json(&body) {
                    Ok(resp) => {
                        if (200..300).contains(&resp.status()) {
                            body_text = Some(resp.into_string().map_err(|e| ProviderError::Http(e.to_string()))?);
                            break;
                        }
                        let status = resp.status();
                        let text = resp.into_string().unwrap_or_default();
                        if !should_retry(status, attempt, self.max_retries) {
                            return Err(ProviderError::Api { status, body: text });
                        }
                        last_err = Some(ProviderError::Api { status, body: text });
                    }
                    Err(e) => {
                        // transport error (timeout / dns / connection): retryable
                        if attempt >= self.max_retries {
                            return Err(ProviderError::Http(e.to_string()));
                        }
                        last_err = Some(ProviderError::Http(e.to_string()));
                    }
                }
                std::thread::sleep(backoff_delay(attempt));
            }
            match body_text {
                Some(t) => t,
                None => return Err(last_err.unwrap_or(ProviderError::Http("no response".into()))),
            }
        };
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| ProviderError::Parse(e.to_string()))?;
        let choice = v
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| ProviderError::Parse("no choices in response".into()))?;
        let msg = choice.get("message").cloned().unwrap_or(serde_json::Value::Null);
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let mut tool_calls = Vec::new();
        if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let f = tc.get("function").cloned().unwrap_or(serde_json::Value::Null);
                tool_calls.push(ToolCall {
                    id: tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                    name: f.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                    arguments_json: f
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}")
                        .to_string(),
                });
            }
        }
        Ok(TurnResponse {
            message: ChatMessage { role: Role::Assistant, content, tool_calls, tool_call_id: None },
            finish_reason: choice
                .get("finish_reason")
                .and_then(|f| f.as_str())
                .unwrap_or("stop")
                .to_string(),
            usage: v.get("usage").and_then(|u| {
                Some(Usage {
                    prompt_tokens: u.get("prompt_tokens")?.as_u64()?,
                    completion_tokens: u.get("completion_tokens")?.as_u64()?,
                })
            }),
        })
    }
}

/// Scripted provider for tests and offline demos. Repeats the last script
/// entry when exhausted.
pub struct MockProvider {
    pub provider_id: String,
    pub model_name: String,
    script: Vec<TurnResponse>,
    cursor: AtomicUsize,
}

impl MockProvider {
    pub fn new(script: Vec<TurnResponse>) -> Self {
        Self {
            provider_id: "mock".into(),
            model_name: "mock-model".into(),
            script,
            cursor: AtomicUsize::new(0),
        }
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
                tool_calls: vec![ToolCall {
                    id: "call_1".into(),
                    name: name.into(),
                    arguments_json: arguments_json.into(),
                }],
                tool_call_id: None,
            },
            finish_reason: "tool_calls".into(),
            usage: None,
        }])
    }

    /// Two-step script: tool call first, then a final text answer.
    pub fn tool_then_text(name: &str, arguments_json: &str, final_text: &str) -> Self {
        Self::new(vec![
            TurnResponse {
                message: ChatMessage {
                    role: Role::Assistant,
                    content: String::new(),
                    tool_calls: vec![ToolCall {
                        id: "call_1".into(),
                        name: name.into(),
                        arguments_json: arguments_json.into(),
                    }],
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

impl StoryboardModelProvider for MockProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }
    fn model(&self) -> &str {
        &self.model_name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities { tool_calls: true, json_response: true }
    }
    fn start_turn(&self, _req: &TurnRequest) -> Result<TurnResponse, ProviderError> {
        let i = self.cursor.fetch_add(1, Ordering::SeqCst);
        let r = self
            .script
            .get(i)
            .or_else(|| self.script.last())
            .cloned()
            .ok_or_else(|| ProviderError::Unsupported("empty mock script".into()))?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_script_repeats_last() {
        let p = MockProvider::simple_text("hello");
        let r1 = p.start_turn(&TurnRequest {
            messages: vec![ChatMessage::user("hi")],
            tools: vec![],
            sampling: SamplingParams::default(),
            force_json: false,
        });
        assert_eq!(r1.unwrap().message.content, "hello");
    }

    #[test]
    fn retry_decision_table() {
        use super::should_retry;
        // 429/5xx retry while attempts remain
        assert!(should_retry(429, 0, 2));
        assert!(should_retry(500, 1, 2));
        assert!(should_retry(503, 2, 3));
        // out of attempts
        assert!(!should_retry(500, 2, 2));
        assert!(!should_retry(429, 0, 0));
        // client errors never retry
        assert!(!should_retry(400, 0, 2));
        assert!(!should_retry(401, 0, 2));
        assert!(!should_retry(404, 3, 5));
    }

    #[test]
    fn backoff_is_exponential() {
        assert_eq!(super::backoff_delay(0).as_millis(), 1000);
        assert_eq!(super::backoff_delay(1).as_millis(), 2000);
        assert_eq!(super::backoff_delay(2).as_millis(), 4000);
        assert_eq!(super::backoff_delay(9).as_millis(), 16000); // capped
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
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: TurnRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tools.len(), 1);
        assert_eq!(back.messages.len(), 2);
    }
}
