use crate::config::LlmSettings;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    timeout: std::time::Duration,
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
    pub fn new(settings: &LlmSettings) -> Self {
        Self {
            client: Client::new(),
            base_url: settings.base_url.trim_end_matches('/').to_string(),
            api_key: settings.api_key.clone(),
            model: settings.model.clone(),
            timeout: settings.timeout(),
        }
    }

    pub async fn extract_json(&self, system: &str, user: &str) -> AppResult<serde_json::Value> {
        let req = ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage { role: "system", content: system },
                ChatMessage { role: "user", content: user },
            ],
            temperature: 0.2,
            response_format: Some(ResponseFormat { ty: "json_object".into() }),
        };

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .timeout(self.timeout)
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
