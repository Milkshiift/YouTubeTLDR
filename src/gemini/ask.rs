use super::error::GeminiResponseError;
use super::types::request::{GeminiRequestBody, SystemInstruction};
use super::types::response::GeminiResponse;
use super::types::sessions::Session;
use serde_json::Value;
use std::time::Duration;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Clone, Default, Debug)]
pub struct Gemini {
    api_key: String,
    model: String,
    sys_prompt: Option<SystemInstruction>,
    generation_config: Option<Value>,
    timeout: Option<Duration>,
}

impl Gemini {
    /// # Arguments
    /// `api_key` get one from [Google AI studio](https://aistudio.google.com/app/apikey)
    /// `model` should be of those mentioned [here](https://ai.google.dev/gemini-api/docs/models#model-variations) in bold black color
    /// `sys_prompt` should follow [gemini doc](https://ai.google.dev/gemini-api/docs/text-generation#image-input)
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        sys_prompt: Option<SystemInstruction>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            sys_prompt,
            generation_config: None,
            timeout: None,
        }
    }

    pub fn ask(&self, session: &mut Session) -> Result<GeminiResponse, GeminiResponseError> {
        let req_url = format!(
            "{BASE_URL}/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let history = session.get_history();
        let body = GeminiRequestBody::new(
            self.sys_prompt.as_ref(),
            &history,
            self.generation_config.as_ref(),
        );

        let mut request = minreq::post(req_url).with_json(&body).map_err(GeminiResponseError::MinreqError)?;
        if let Some(timeout) = self.timeout {
            request = request.with_timeout(timeout.as_secs());
        }

        let response = request.send().map_err(GeminiResponseError::MinreqError)?;

        if response.status_code < 200 || response.status_code > 299 {
            let text = response.as_str().unwrap_or("").to_string();
            return Err(GeminiResponseError::StatusNotOk(text));
        }

        let reply = GeminiResponse::new(response).map_err(GeminiResponseError::MinreqError)?;
        session.update(&reply);
        Ok(reply)
    }
}