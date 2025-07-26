mod gemini;
mod subtitle;

use crate::gemini::ask::Gemini;
use crate::gemini::types::request::SystemInstruction;
use crate::gemini::types::sessions::Session;
use crate::subtitle::{get_video_data, merge_transcript, MergeConfig};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    dry_run: bool,
    transcript_only: bool
}

#[derive(Serialize)]
struct SummarizeResponse {
    summary: String,
    subtitles: String,
    video_name: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn main() {
    let ip = env::var("TLDR_IP").unwrap_or("0.0.0.0".to_string());
    let port = env::var("TLDR_PORT").unwrap_or("8000".to_string());
    let addr = format!("{ip}:{port}");

    let num_workers = env::var("TLDR_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let server = Server::http(&addr).expect("❌ Failed to start server!");
    println!("✅ Server started at http://{}", server.server_addr());
    println!("✅ Spawning {num_workers} worker threads.");

    // Create a channel to act as a job queue for requests.
    let (sender, receiver) = mpsc::channel::<Request>();
    let receiver = Arc::new(Mutex::new(receiver));

    for id in 0..num_workers {
        let receiver_clone = Arc::clone(&receiver);
        thread::spawn(move || {
            println!("   Worker {id} started.");
            loop {
                // Lock the mutex to get a job from the queue.
                let job = receiver_clone.lock().unwrap().recv();

                match job {
                    Ok(request) => {
                        println!("   Worker {} handling request for: {}", id, request.url());
                        handle_request(request);
                    }
                    Err(_) => {
                        // The channel has closed, so the main thread has shut down.
                        println!("   Worker {id} shutting down.");
                        break;
                    }
                }
            }
        });
    }

    println!("▶️ Ready to accept requests.");
    for request in server.incoming_requests() {
        let is_static_and_get = matches!(
            (request.method(), request.url()), 
            (Method::Get, "/") | (Method::Get, "/index.html") | (Method::Get, "/style.css") | (Method::Get, "/script.js")
        );

        if is_static_and_get {
            // Handle static requests on the main thread since they are fast
            handle_request(request);
        } else {
            // Summarization or other POST requests are sent to the worker pool, they are slow
            if sender.send(request).is_err() {
                eprintln!("❌ Failed to send request to worker pool.");
                break;
            }
        }
    }
}

fn handle_request(request: Request) {
    match (request.method(), request.url()) {
        (Method::Post, "/api/summarize") => handle_summarize(request),
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            serve_static(request, include_str!("../static/index.html"), "text/html");
        }
        (Method::Get, "/style.css") => {
            serve_static(request, include_str!("../static/style.css"), "text/css");
        }
        (Method::Get, "/script.js") => {
            serve_static(request, include_str!("../static/script.js"), "application/javascript");
        }
        _ => {
            respond_with_error(request, "404 Not Found", StatusCode(404));
        }
    }
}

fn handle_summarize(mut request: Request) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        respond_with_error(request, "Failed to read request body", StatusCode(400));
        return;
    }

    let summarize_request: SummarizeRequest = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(e) => {
            respond_with_error(request, &format!("Invalid JSON: {e}"), StatusCode(400));
            return;
        }
    };

    let result = perform_summary_work(summarize_request);

    match result {
        Ok(response_data) => {
            let json_response = serde_json::to_string(&response_data).unwrap();
            let response = Response::from_string(json_response)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                )
                .with_status_code(200);

            if let Err(e) = request.respond(response) {
                eprintln!("⚠️ Could not send success response: {e}");
            }
        }
        Err(error_message) => {
            eprintln!("Worker failed with error: {error_message}");
            respond_with_error(request, &error_message, StatusCode(500));
        }
    }
}

fn perform_summary_work(req: SummarizeRequest) -> Result<SummarizeResponse, String> {
    if req.dry_run {
        let test_md = include_str!("./markdown_test.md").to_string();
        return Ok(SummarizeResponse {
            summary: test_md.clone(),
            subtitles: test_md,
            video_name: "Dry Run".to_string(),
        });
    }

    let (transcript, video_name) = get_video_data(&req.url, "en")
        .map_err(|e| format!("Failed to get YouTube transcript: {e}"))?;

    let merged_transcript = merge_transcript(
        &transcript,
        &MergeConfig {
            paragraph_pause_threshold_secs: 1.5,
            remove_annotations: false,
        },
    );
    
    if req.transcript_only {
        return Ok(SummarizeResponse {
            summary: merged_transcript.clone(),
            subtitles: merged_transcript,
            video_name,
        });
    }

    let api_key = req.api_key.filter(|k| !k.is_empty()).ok_or("API key not provided")?;
    let model = req.model.filter(|m| !m.is_empty()).ok_or("Model unspecified")?;
    let system_prompt = req.system_prompt.filter(|p| !p.is_empty()).ok_or("System prompt unspecified")?;

    let gemini = Gemini::new(&api_key, model, Some(SystemInstruction::from_str(&system_prompt)));
    let mut session = Session::new(2);
    session.ask_string(merged_transcript.clone());

    let summary = gemini
        .ask(&mut session)
        .map_err(|e| format!("Gemini API request failed: {e}"))?
        .get_text("");

    Ok(SummarizeResponse {
        summary,
        subtitles: merged_transcript,
        video_name,
    })
}


fn serve_static(request: Request, content: &'static str, content_type: &'static str) {
    let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
    let response = Response::from_string(content).with_header(header);

    if let Err(e) = request.respond(response) {
        eprintln!("⚠️ Could not send static file response: {e}");
    }
}

fn respond_with_error(request: Request, message: &str, status_code: StatusCode) {
    let error_response = ErrorResponse {
        error: message.to_string(),
    };

    let json_response = serde_json::to_string(&error_response).unwrap();

    let response = Response::from_string(json_response)
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
        .with_status_code(status_code);

    if let Err(e) = request.respond(response) {
        eprintln!("⚠️ Could not send error response: {e}");
    }
}