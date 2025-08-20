use serde::Deserialize;
use serde_json::json;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[allow(dead_code)]
#[derive(Debug)]
pub enum Error {
    Request(minreq::Error),
    StatusNotOk(String),
    NoTextInResponse,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

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

pub fn summarize(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    transcript: &str,
) -> Result<String, Error> {
    let req_url = format!("{BASE_URL}/{}:generateContent?key={api_key}", model);

    let body = json!({
        "system_instruction": {
            "parts": [
                { "text": system_prompt }
            ]
        },
        "contents": [
            {
                "role": "user",
                "parts": [
                    { "text": transcript }
                ]
            }
        ],
        "generationConfig": {
            "temperature": 1,
            "topK": 64,
            "topP": 0.95,
            "maxOutputTokens": 65536,
            "stopSequences": []
        },
        "safetySettings": [
            {
                "category": "HARM_CATEGORY_HARASSMENT",
                "threshold": "BLOCK_NONE"
            },
            {
                "category": "HARM_CATEGORY_HATE_SPEECH",
                "threshold": "BLOCK_NONE"
            },
            {
                "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                "threshold": "BLOCK_NONE"
            },
            {
                "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
                "threshold": "BLOCK_NONE"
            }
        ]
    });

    let response = minreq::post(req_url)
        .with_timeout(45) // 45 second timeout, summarization can be slow
        .with_json(&body)
        .map_err(Error::Request)?
        .send()
        .map_err(Error::Request)?;

    if response.status_code < 200 || response.status_code > 299 {
        let text = response.as_str().unwrap_or("").to_string();
        return Err(Error::StatusNotOk(text));
    }

    let reply: GeminiResponse = response.json().map_err(Error::Request)?;

    reply.candidates
        .get(0)
        .and_then(|c| c.content.parts.get(0))
        .map(|p| p.text.clone())
        .ok_or(Error::NoTextInResponse)
}
