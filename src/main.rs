mod gemini;
mod subtitle;

use tiny_http::{Server, Response, Request, StatusCode, Header};
use serde::{Deserialize, Serialize};
use std::{env, thread};
use std::sync::LazyLock;
use crate::gemini::ask::Gemini;
use crate::gemini::types::request::SystemInstruction;
use crate::gemini::types::sessions::Session;
use crate::subtitle::{get_youtube_transcript, merge_transcript, MergeConfig};

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    dry_run: bool
}

#[derive(Serialize)]
struct SummarizeResponse {
    summary: String,
    subtitles: String
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

static GEMINI_API_KEY: LazyLock<String> = LazyLock::new(|| {
    env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY not set")
});

fn main() {
    let server = Server::http("0.0.0.0:8000").unwrap();
    println!("✅ Server started at http://localhost:8000");

    for request in server.incoming_requests() {
        let url = request.url();
        println!("Received request for URL: {}", url);

        match url {
            "/api/summarize" => handle_summarize(request),
            "/" | "/index.html" => {
                serve_static_response(
                    request,
                    include_str!("../static/index.html"),
                    "text/html"
                );
            },
            "/style.css" => {
                serve_static_response(
                    request,
                    include_str!("../static/style.css"),
                    "text/css"
                );
            },
            "/script.js" => {
                serve_static_response(
                    request,
                    include_str!("../static/script.js"),
                    "application/javascript"
                );
            },
            _ => {
                let response = Response::from_string("404 Not Found")
                    .with_status_code(StatusCode(404));
                request.respond(response).unwrap();
            }
        }
    }
}

fn serve_static_response(request: Request, content: &'static str, content_type: &'static str) {
    let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();

    let response = Response::from_string(content).with_header(header);

    request.respond(response).unwrap();
}

fn handle_summarize(mut request: Request) {
    if request.method().as_str() != "POST" {
        respond_with_error(request, "Method Not Allowed", StatusCode(405));
        return;
    }

    let mut body = String::new();
    if let Err(_) = request.as_reader().read_to_string(&mut body) {
        respond_with_error(request, "Failed to read request body", StatusCode(400));
        return;
    }

    let summarize_request: SummarizeRequest = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(e) => {
            let error_message = format!("Invalid JSON format: {}", e);
            respond_with_error(request, &error_message, StatusCode(400));
            return;
        }
    };

    let job_handle = thread::spawn(move || -> Result<(String, String), String> {
        if summarize_request.dry_run {
            let test = include_str!("./markdown_test.md").to_string();
            return Ok((test.clone(), test));
        }

        let transcript = get_youtube_transcript(&summarize_request.url, "en")
            .map_err(|e| format!("Failed to get YouTube transcript: {}", e))?;

        let merged_transcript = merge_transcript(&transcript, &MergeConfig {
            paragraph_pause_threshold_secs: 2.0,
            remove_annotations: false,
        });

        let api_key = summarize_request.api_key.filter(|k| !k.is_empty()).ok_or("API key not provided")?;
        let model = summarize_request.model.unwrap_or_else(|| "gemini-2.5-flash".to_string());
        let system_prompt = summarize_request.system_prompt.unwrap_or_else(|| "You are an expert video summarizer specializing in creating structured, accurate overviews. Given a YouTube video transcript, extract and present the most crucial information in an article-style format. Prioritize fidelity to the original content, ensuring all significant points, arguments, and key details are faithfully represented. Organize the summary logically with clear, descriptive headings and/or concise bullet points. For maximum skim-readability, bold key terms, core concepts, and critical takeaways within the text. Eliminate conversational filler, repeated phrases, and irrelevant tangents, but retain all essential content.".to_string());

        let gemini = Gemini::new(&api_key, model, Some(SystemInstruction::from_str(&system_prompt)));
        let mut session = Session::new(2);
        session.ask_string(merged_transcript.clone());
        
        let summary = gemini.ask(&mut session)
            .map_err(|e| format!("Gemini API request failed: {}", e))?
            .get_text("");

        Ok((summary, merged_transcript))
    });

    match job_handle.join() {
        Ok(Ok((summary_text, merged_transcript))) => {
            let success_response = SummarizeResponse { summary: summary_text, subtitles: merged_transcript };
            let json_response = serde_json::to_string(&success_response).unwrap();

            let response = Response::from_string(json_response)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                .with_status_code(StatusCode(200));

            request.respond(response).unwrap();
        }
        Ok(Err(error_message)) => {
            eprintln!("Worker thread failed with error: {}", error_message);
            respond_with_error(request, &error_message, StatusCode(500));
        }
        Err(_) => {
            eprintln!("Worker thread panicked unexpectedly.");
            respond_with_error(request, "A critical error occurred in the worker thread.", StatusCode(500));
        }
    }
}

fn respond_with_error(request: Request, message: &str, status_code: StatusCode) {
    let error_response = ErrorResponse { error: message.to_string() };
    let json_response = serde_json::to_string(&error_response).unwrap();

    let response = Response::from_string(json_response)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
        .with_status_code(status_code);

    request.respond(response).unwrap();
}