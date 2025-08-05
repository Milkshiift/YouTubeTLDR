mod gemini;
mod subtitle;

use crate::gemini::ask::Gemini;
use crate::gemini::types::request::SystemInstruction;
use crate::gemini::types::sessions::Session;
use crate::subtitle::get_video_data;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::thread;

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    dry_run: bool,
    transcript_only: bool,
}

#[derive(Serialize)]
struct SummarizeResponse {
    summary: String,
    subtitles: String,
    video_name: String,
}

// Represents a work item for the worker pool
struct WorkItem {
    stream: TcpStream,
    request_data: Vec<u8>,
}

macro_rules! define_static_response {
    ($name:ident, $content_type:expr, $path:expr) => {
        static $name: LazyLock<Vec<u8>> = LazyLock::new(|| {
            build_response(
                b"200 OK",
                $content_type,
                include_bytes!(concat!("../static/", $path)),
            )
        });
    };
}

define_static_response!(HTML_RESPONSE, b"text/html; charset=utf-8", "index.html");
define_static_response!(CSS_RESPONSE, b"text/css; charset=utf-8", "style.min.css");
define_static_response!(JS_RESPONSE, b"application/javascript; charset=utf-8", "script.min.js");

static NOT_FOUND_RESPONSE: LazyLock<Vec<u8>> = LazyLock::new(|| {
    build_response(b"404 NOT FOUND", b"text/plain", b"404")
});

fn main() {
    let ip = env::var("TLDR_IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("TLDR_PORT").unwrap_or_else(|_| "8000".to_string());
    let addr = format!("{}:{}", ip, port);

    let num_workers = env::var("TLDR_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let listener = TcpListener::bind(&addr).expect("❌ Failed to start server!");
    println!("✅ Server started at http://{}", addr);
    println!("✅ Spawning {} worker threads.", num_workers);

    // Create worker pool
    let (sender, receiver) = mpsc::channel::<WorkItem>();
    let receiver = Arc::new(Mutex::new(receiver));

    // Spawn worker threads
    for id in 0..num_workers {
        let receiver_clone = Arc::clone(&receiver);
        thread::spawn(move || {
            println!("   Worker {} started.", id);
            loop {
                // Lock the mutex to get a job from the queue
                let job = receiver_clone.lock().unwrap().recv();

                match job {
                    Ok(work_item) => {
                        handle_worker_request(id, work_item);
                    }
                    Err(_) => {
                        // The channel has closed, so the main thread has shut down
                        println!("   Worker {} shutting down.", id);
                        break;
                    }
                }
            }
        });
    }

    println!("▶️ Ready to accept requests.");

    // Main server loop
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(e) = handle_connection(&mut stream, &sender) {
                    eprintln!("❌ Error handling connection: {}", e);
                }
            }
            Err(e) => {
                eprintln!("❌ Connection failed: {}", e);
            }
        }
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    sender: &mpsc::Sender<WorkItem>,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 1024];
    let bytes_read = stream.read(&mut buffer)?;
    let request_data = buffer[..bytes_read].to_vec();

    if request_data.starts_with(b"GET ") {
        // Handle GET requests immediately (they're fast)
        handle_get_request(&request_data, stream)
    } else if request_data.starts_with(b"POST /api/summarize") {
        // Send POST requests to worker pool (they're slow)
        let work_item = WorkItem {
            stream: stream.try_clone()?,
            request_data,
        };

        if let Err(_) = sender.send(work_item) {
            eprintln!("❌ Failed to send work to worker pool");
            write_error_response(stream, "500 Internal Server Error", "Worker pool unavailable")?;
        }

        Ok(())
    } else {
        stream.write_all(&NOT_FOUND_RESPONSE)?;
        stream.flush()
    }
}

fn handle_worker_request(worker_id: usize, mut work_item: WorkItem) {
    println!("   Worker {} handling summarize request", worker_id);

    if let Err(e) = handle_summarize_request(&work_item.request_data, &mut work_item.stream) {
        eprintln!("❌ Worker {} error handling request: {}", worker_id, e);
        let _ = write_error_response(&mut work_item.stream, "500 Internal Server Error", "Request processing failed");
    }
}

fn handle_get_request(buffer: &[u8], stream: &mut TcpStream) -> std::io::Result<()> {
    // Find the path in the request line
    let request_line = buffer
        .split(|&b| b == b'\r' || b == b'\n')
        .next()
        .unwrap_or(buffer);

    let parts: Vec<&[u8]> = request_line.splitn(3, |&b| b == b' ').collect();
    let path = parts.get(1).copied().unwrap_or(b"/");

    let response_bytes = match path {
        b"/" | b"/index.html" => &*HTML_RESPONSE,
        b"/style.min.css" => &*CSS_RESPONSE,
        b"/script.min.js" => &*JS_RESPONSE,
        _ => &*NOT_FOUND_RESPONSE,
    };

    stream.write_all(response_bytes)?;
    stream.flush()
}

fn handle_summarize_request(buffer: &[u8], stream: &mut TcpStream) -> std::io::Result<()> {
    let content_length = find_content_length(buffer).unwrap_or(0);
    if content_length == 0 {
        return write_error_response(stream, "400 BAD REQUEST", "Missing or invalid Content-Length");
    }

    // Read the request body
    let body = read_request_body(buffer, stream, content_length)?;

    // Parse JSON safely
    let json_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return write_error_response(stream, "400 BAD REQUEST", "Invalid UTF-8 in request body");
        }
    };

    let summarize_request: SummarizeRequest = match serde_json::from_str(json_str) {
        Ok(req) => req,
        Err(e) => {
            eprintln!("JSON parse error: {}", e);
            return write_error_response(stream, "400 BAD REQUEST", "Invalid JSON request");
        }
    };

    // Process the request
    match perform_summary_work(summarize_request) {
        Ok(response_data) => {
            let json_response = serde_json::to_string(&response_data)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

            let response = build_response(b"200 OK", b"application/json", json_response.as_bytes());
            stream.write_all(&response)?;
            stream.flush()
        }
        Err(error_message) => {
            eprintln!("Summary work failed: {}", error_message);
            write_error_response(stream, "500 Internal Server Error", &error_message)
        }
    }
}

fn read_request_body(
    buffer: &[u8],
    stream: &mut TcpStream,
    content_length: usize,
) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::with_capacity(content_length);

    // Check if we already have some body data in the buffer
    if let Some(body_start_index) = find_body_start(buffer) {
        let body_in_buffer = &buffer[body_start_index..];
        body.extend_from_slice(body_in_buffer);
    }

    // Read the rest of the body if needed
    if body.len() < content_length {
        let remaining = content_length - body.len();
        let mut temp_buffer = vec![0u8; remaining];
        stream.read_exact(&mut temp_buffer)?;
        body.extend_from_slice(&temp_buffer);
    }

    Ok(body)
}

fn write_error_response(
    stream: &mut TcpStream,
    status: &str,
    message: &str,
) -> std::io::Result<()> {
    let error_json = format!(r#"{{"error": "{}"}}"#, message.replace('"', "\\\""));
    let response = build_response(status.as_bytes(), b"application/json", error_json.as_bytes());
    stream.write_all(&response)?;
    stream.flush()
}

fn build_response(status: &[u8], content_type: &[u8], content: &[u8]) -> Vec<u8> {
    let content_length_str = content.len().to_string();
    let mut response = Vec::with_capacity(
        b"HTTP/1.1 \r\nContent-Type: \r\nContent-Length: \r\n\r\n".len()
            + status.len()
            + content_type.len()
            + content_length_str.len()
            + content.len(),
    );

    response.extend_from_slice(b"HTTP/1.1 ");
    response.extend_from_slice(status);
    response.extend_from_slice(b"\r\nContent-Type: ");
    response.extend_from_slice(content_type);
    response.extend_from_slice(b"\r\nContent-Length: ");
    response.extend_from_slice(content_length_str.as_bytes());
    response.extend_from_slice(b"\r\n\r\n");
    response.extend_from_slice(content);

    response
}

fn find_content_length(buffer: &[u8]) -> Option<usize> {
    let headers = std::str::from_utf8(buffer).ok()?;
    for line in headers.lines() {
        if line.to_ascii_lowercase().starts_with("content-length:") {
            return line.split(':').nth(1)?.trim().parse::<usize>().ok();
        }
    }
    None
}

fn find_body_start(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len().saturating_sub(3) {
        if buffer[i..i + 4] == *b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
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
        .map_err(|e| format!("Failed to get YouTube transcript: {}", e))?;

    if req.transcript_only {
        return Ok(SummarizeResponse {
            summary: transcript.clone(),
            subtitles: transcript,
            video_name,
        });
    }

    let api_key = req
        .api_key
        .filter(|k| !k.is_empty())
        .ok_or("API key not provided")?;
    let model = req
        .model
        .filter(|m| !m.is_empty())
        .ok_or("Model unspecified")?;
    let system_prompt = req
        .system_prompt
        .filter(|p| !p.is_empty())
        .ok_or("System prompt unspecified")?;

    let gemini = Gemini::new(&api_key, model, Some(SystemInstruction::from_str(&system_prompt)));
    let mut session = Session::new(2);
    session.ask_string(transcript.clone());

    let summary = gemini
        .ask(&mut session)
        .map_err(|e| format!("Gemini API request failed: {}", e))?
        .get_text("");

    Ok(SummarizeResponse {
        summary,
        subtitles: transcript,
        video_name,
    })
}