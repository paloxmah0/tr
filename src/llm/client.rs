use crate::dynamic_config::SharedConfig;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
    config: SharedConfig,
    timeout_secs: u64,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatRespMessage,
}

#[derive(Deserialize)]
struct ChatRespMessage {
    content: String,
}

impl LlmClient {
    pub fn new(config: SharedConfig, timeout_secs: u64) -> Self {
        Self {
            client: Client::new(),
            config,
            timeout_secs,
        }
    }

    async fn cfg(&self, key: &str) -> String {
        self.config.read().await.get(key).cloned().unwrap_or_default()
    }

    pub async fn extract_json(&self, system: &str, user: &str) -> AppResult<serde_json::Value> {
        let base_url = self.cfg(crate::dynamic_config::keys::LLM_BASE_URL).await;
        let api_key = self.cfg(crate::dynamic_config::keys::LLM_API_KEY).await;
        let model = self.cfg(crate::dynamic_config::keys::LLM_MODEL).await;
        let model = if model.is_empty() { "gpt-4o-mini".to_string() } else { model };

        if api_key.is_empty() {
            return Err(AppError::Llm("no LLM API key set — configure it in Settings".into()));
        }
        if base_url.is_empty() {
            return Err(AppError::Llm("no LLM base URL set — configure it in Settings".into()));
        }

        let req = ChatRequest {
            model: &model,
            messages: vec![
                ChatMessage { role: "system", content: system },
                ChatMessage { role: "user", content: user },
            ],
            temperature: 0.2,
            response_format: Some(ResponseFormat { ty: "json_object".into() }),
        };

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&api_key)
            .timeout(Duration::from_secs(self.timeout_secs))
            .json(&req)
            .send()
            .await
            .map_err(|e| AppError::Llm(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Llm(format!("LLM HTTP {status}: {body}")));
        }

        let chat: ChatResponse = resp.json().await.map_err(|e| AppError::Llm(e.to_string()))?;
        let content = chat
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Llm("no choices in LLM response".into()))?
            .message
            .content;

        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| AppError::Llm(format!("invalid JSON from LLM: {e}")))?;
        Ok(value)
    }
}
