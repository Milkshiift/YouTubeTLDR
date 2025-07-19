mod gemini;
mod subtitle;

use tiny_http::{Server, Response, Request, StatusCode, Header};
use serde::{Deserialize, Serialize};
use std::{env, thread};
use std::sync::LazyLock;
use crate::subtitle::{get_youtube_transcript, merge_transcript, MergeConfig};

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
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
        Err(_) => {
            respond_with_error(request, "Invalid JSON format", StatusCode(400));
            return;
        }
    };
    
    let job_handle = thread::spawn(move || {
        let transcript = get_youtube_transcript(&summarize_request.url, "en").unwrap();
        
        let merged_transcript = merge_transcript(&transcript, &MergeConfig {
            paragraph_pause_threshold_secs: 1.5,
            remove_annotations: false,
        });
        
        let summary = format!("This is a brilliant summary of the transcript: '{}'", merged_transcript);

        (summary, merged_transcript)
    });

    match job_handle.join() {
        Ok((summary_text, merged_transcript)) => {
            let success_response = SummarizeResponse { summary: summary_text, subtitles: merged_transcript };
            let json_response = serde_json::to_string(&success_response).unwrap();

            let response = Response::from_string(json_response)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                .with_status_code(StatusCode(200));

            request.respond(response).unwrap();
        }
        Err(_) => {
            respond_with_error(request, "Worker thread failed", StatusCode(500));
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