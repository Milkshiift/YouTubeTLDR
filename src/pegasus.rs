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
            Self::Request(_) => write!(f, "Failed to send request to the TwelveLabs API"),
            Self::Api { status, body } => {
                write!(
                    f,
                    "TwelveLabs API returned an error (status {status}): {body}"
                )
            }
            Self::Json(_) => write!(f, "Failed to parse a response from the TwelveLabs API"),
            Self::NoTextInResponse => write!(f, "The API response did not contain any text"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Api { .. } | Self::NoTextInResponse => None,
        }
    }
}

const ANALYZE_URL: &str = "https://api.twelvelabs.io/v1.3/analyze";

#[derive(Deserialize)]
struct AnalyzeResponse {
    data: String,
}

#[derive(Serialize)]
struct AnalyzeRequest<'a> {
    video: VideoSource<'a>,
    model_name: &'a str,
    prompt: &'a str,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize)]
struct VideoSource<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    url: &'a str,
}

/// Summarize a video directly from its pixels using `TwelveLabs`' Pegasus model.
///
/// Unlike the transcript-based Gemini path, Pegasus watches the video itself, so
/// `video_url` must be a direct link to a raw media file (e.g. an `.mp4`).
/// `YouTube` watch/share links are not accepted by the API.
pub fn summarize(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    video_url: &str,
) -> Result<String, Error> {
    let request_body = AnalyzeRequest {
        video: VideoSource {
            kind: "url",
            url: video_url,
        },
        model_name: model,
        prompt: system_prompt,
        temperature: 0.2,
        max_tokens: 2048,
        stream: false,
    };

    let body_str = json::to_vec(&request_body);

    let response = minreq::post(ANALYZE_URL)
        .with_header("x-api-key", api_key)
        .with_header("Content-Type", "application/json")
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

    let reply: AnalyzeResponse = json::from_slice(response.as_bytes()).map_err(Error::Json)?;

    if reply.data.is_empty() {
        return Err(Error::NoTextInResponse);
    }

    Ok(reply.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    // No-network: lock the request wire format the Analyze API requires
    // (`video.type` = "url", non-streaming response).
    #[test]
    fn serializes_request_in_analyze_shape() {
        let body = json::to_string(&AnalyzeRequest {
            video: VideoSource {
                kind: "url",
                url: "https://example.com/v.mp4",
            },
            model_name: "pegasus1.5",
            prompt: "Summarize this.",
            temperature: 0.2,
            max_tokens: 2048,
            stream: false,
        });

        assert!(body.contains(r#""type":"url""#));
        assert!(body.contains(r#""url":"https://example.com/v.mp4""#));
        assert!(body.contains(r#""model_name":"pegasus1.5""#));
        assert!(body.contains(r#""stream":false"#));
    }

    #[test]
    fn parses_non_streaming_response() {
        let reply: AnalyzeResponse =
            json::from_str(r#"{"data":"A squirrel emerges from a tree.","finish_reason":"stop"}"#)
                .unwrap();
        assert_eq!(reply.data, "A squirrel emerges from a tree.");
    }

    // Live: opt-in smoke test, skipped unless TWELVELABS_API_KEY is set.
    // Run with: cargo test -- --ignored
    #[test]
    #[ignore]
    fn live_pegasus_summary() {
        let Ok(api_key) = std::env::var("TWELVELABS_API_KEY") else {
            return;
        };
        let summary = summarize(
            &api_key,
            "pegasus1.5",
            "In one sentence, what happens in this video?",
            "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_1MB.mp4",
        )
        .expect("Pegasus summary");
        assert!(!summary.is_empty());
    }
}
