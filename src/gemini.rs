use miniserde::{Deserialize, Serialize, json};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Request(minreq::Error),
    Api { status: u16, body: String },
    Json(miniserde::Error),
    NoTextInResponse,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Request(_) => write!(f, "Failed to send request to the Gemini API"),
            Error::Api { status, body } => {
                write!(f, "Gemini API returned an error (status {status}): {body}")
            }
            Error::Json(_) => write!(f, "Failed to parse a response from the Gemini API"),
            Error::NoTextInResponse => write!(f, "The API response did not contain any text"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Request(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Api { .. } | Error::NoTextInResponse => None,
        }
    }
}

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: ContentResponse,
}

#[derive(Deserialize)]
struct ContentResponse {
    parts: Vec<PartResponse>,
}

#[derive(Deserialize)]
struct PartResponse {
    text: String,
}

#[derive(Serialize)]
struct GeminiRequest<'a> {
    system_instruction: SystemInstruction<'a>,
    contents: Vec<ContentRequest<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig<'a>,
    #[serde(rename = "safetySettings")]
    safety_settings: Vec<SafetySetting<'a>>,
}

#[derive(Serialize)]
struct SystemInstruction<'a> {
    parts: Vec<PartRequest<'a>>,
}

#[derive(Serialize)]
struct ContentRequest<'a> {
    role: &'a str,
    parts: Vec<PartRequest<'a>>,
}

#[derive(Serialize)]
struct PartRequest<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenerationConfig<'a> {
    temperature: f32,
    #[serde(rename = "topK")]
    top_k: u32,
    #[serde(rename = "topP")]
    top_p: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "stopSequences")]
    stop_sequences: Vec<&'a str>,
}

#[derive(Serialize)]
struct SafetySetting<'a> {
    category: &'a str,
    threshold: &'a str,
}

pub fn summarize(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    transcript: &str,
) -> Result<String, Error> {
    let req_url = format!("{BASE_URL}/{}:generateContent?key={api_key}", model);

    let request_body = GeminiRequest {
        system_instruction: SystemInstruction {
            parts: vec![PartRequest {
                text: system_prompt,
            }],
        },
        contents: vec![ContentRequest {
            role: "user",
            parts: vec![PartRequest { text: transcript }],
        }],
        generation_config: GenerationConfig {
            temperature: 1.0,
            top_k: 64,
            top_p: 0.95,
            max_output_tokens: 65536,
            stop_sequences: vec![],
        },
        safety_settings: vec![
            SafetySetting {
                category: "HARM_CATEGORY_HARASSMENT",
                threshold: "BLOCK_NONE",
            },
            SafetySetting {
                category: "HARM_CATEGORY_HATE_SPEECH",
                threshold: "BLOCK_NONE",
            },
            SafetySetting {
                category: "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                threshold: "BLOCK_NONE",
            },
            SafetySetting {
                category: "HARM_CATEGORY_DANGEROUS_CONTENT",
                threshold: "BLOCK_NONE",
            },
        ],
    };

    let body_str = json::to_vec(&request_body);

    let response = minreq::post(req_url)
        .with_timeout(120)
        .with_body(body_str)
        .send()
        .map_err(Error::Request)?;

    if !(200..=299).contains(&response.status_code) {
        let body = response.as_str().unwrap_or("No response body").to_string();
        return Err(Error::Api {
            status: response.status_code as u16,
            body,
        });
    }

    let reply: GeminiResponse = json::from_slice(response.as_bytes()).map_err(Error::Json)?;

    reply
        .candidates
        .first()
        .and_then(|c| c.content.parts.first())
        .map(|p| p.text.clone())
        .ok_or(Error::NoTextInResponse)
}
