mod gemini;
mod subtitle;

use crate::gemini::ask::Gemini;
use crate::gemini::types::request::SystemInstruction;
use crate::gemini::types::sessions::Session;
use crate::subtitle::get_video_data;
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
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

struct WorkItem {
    stream: TcpStream,
}

macro_rules! static_response {
    ($name:ident, $content_type:expr, $path:expr) => {
        static $name: &[u8] = include_bytes!(concat!("../static/", $path));
    };
}

static_response!(HTML_RESPONSE, b"text/html; charset=utf8", "index.html");
static_response!(CSS_RESPONSE, b"text/css; charset=utf8", "style.min.css");
static_response!(JS_RESPONSE, b"application/javascript; charset=utf8", "script.min.js");

const NOT_FOUND_RESPONSE: &[u8] = b"HTTP/1.1 404 NOT FOUND\r\nContent-Type: text/plain\r\nContent-Length: 3\r\n\r\n404";

fn main() -> io::Result<()> {
    let ip = env::var("TLDR_IP").unwrap_or_else(|_| "0.0.0.0".into());
    let port = env::var("TLDR_PORT").unwrap_or_else(|_| "8000".into());
    let addr = format!("{}:{}", ip, port);

    let num_workers = env::var("TLDR_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let listener = TcpListener::bind(&addr)?;
    println!("✅ Server started at http://{}", addr);
    println!("✅ Spawning {} worker threads", num_workers);
    
    let (sender, receiver) = bounded(100);
    
    for id in 0..num_workers {
        let receiver = receiver.clone();
        thread::spawn(move || worker(id, receiver));
    }

    println!("▶️ Ready to accept requests");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle_connection(stream, &sender) {
                    eprintln!("❌ Connection error: {}", e);
                }
            }
            Err(e) => eprintln!("❌ Accept failed: {}", e),
        }
    }
    Ok(())
}

fn handle_connection(stream: TcpStream, sender: &Sender<WorkItem>) -> io::Result<()> {
    let mut stream_clone = stream.try_clone()?;

    let work_item = WorkItem { stream };
    
    match sender.try_send(work_item) {
        Ok(()) => Ok(()),
        Err(crossbeam_channel::TrySendError::Full(_)) => {
            write_error_response(&mut stream_clone, "503", "Server busy")
        }
        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
            write_error_response(&mut stream_clone, "500", "Worker pool down")
        }
    }
}

fn worker(id: usize, receiver: Receiver<WorkItem>) {
    println!("   Worker {} started", id);
    loop {
        match receiver.recv() {
            Ok(mut work_item) => {
                if let Err(e) = handle_request(&mut work_item.stream) {
                    eprintln!("❌ Worker {} error: {}", id, e);
                    let _ = write_error_response(&mut work_item.stream, "500", &format!("Internal Server Error: {}", e));
                }
            }
            Err(_) => {
                println!("   Worker {} shutting down", id);
                break;
            }
        }
    }
}

fn handle_request(stream: &mut TcpStream) -> Result<(), String> {
    let mut buffer = [0; 2048];
    let bytes_read = stream.read(&mut buffer).map_err(|e| e.to_string())?;

    if bytes_read == 0 {
        return Ok(());
    }

    let request_data = &buffer[..bytes_read];

    if request_data.starts_with(b"GET ") {
        handle_get(request_data, stream.try_clone().unwrap()).map_err(|e| e.to_string())
    } else if request_data.starts_with(b"POST /api/summarize") {
        let (headers, body_start) = parse_headers(request_data).ok_or("Invalid headers")?;
        let content_length = get_content_length(headers).ok_or("Missing Content-Length")?;

        let body = read_body(request_data, body_start, content_length, stream)
            .map_err(|e| e.to_string())?;

        let req: SummarizeRequest = serde_json::from_slice(&body)
            .map_err(|e| format!("JSON error: {}", e))?;

        let response = perform_summary_work(req)
            .and_then(|res| serde_json::to_string(&res).map_err(|e| e.to_string()))
            .map_err(|e| format!("Processing error: {}", e))?;

        let http_response = build_response("200 OK", "application/json", response.as_bytes());
        stream.write_all(&http_response).map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())
    } else {
        stream.write_all(NOT_FOUND_RESPONSE).map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())
    }
}

fn handle_get(request: &[u8], mut stream: TcpStream) -> io::Result<()> {
    let path = request
        .splitn(3, |&b| b == b' ')
        .nth(1)
        .unwrap_or(b"/");

    let response = match path {
        b"/" | b"/index.html" => build_static_response("text/html", HTML_RESPONSE),
        b"/style.min.css" => build_static_response("text/css", CSS_RESPONSE),
        b"/script.min.js" => build_static_response("application/javascript", JS_RESPONSE),
        _ => NOT_FOUND_RESPONSE.to_vec(),
    };

    stream.write_all(&response)?;
    stream.flush()
}

fn perform_summary_work(req: SummarizeRequest) -> Result<SummarizeResponse, String> {
    if req.dry_run {
        let test_md = include_str!("./markdown_test.md").to_string();
        return Ok(SummarizeResponse {
            summary: test_md.clone(),
            subtitles: test_md,
            video_name: "Dry Run".into(),
        });
    }

    let (transcript, video_name) = get_video_data(&req.url, "en")
        .map_err(|e| format!("Transcript error: {}", e))?;

    if req.transcript_only {
        return Ok(SummarizeResponse {
            summary: transcript.clone(),
            subtitles: transcript,
            video_name,
        });
    }

    let api_key = req.api_key.as_deref().filter(|k| !k.is_empty()).ok_or("Missing API key")?;
    let model = req.model.as_deref().filter(|m| !m.is_empty()).ok_or("Missing model")?;
    let system_prompt = req.system_prompt.as_deref().filter(|p| !p.is_empty()).ok_or("Missing prompt")?;

    let gemini = Gemini::new(api_key, model, Some(SystemInstruction::from_str(system_prompt)));
    let mut session = Session::new(2);
    session.ask_string(transcript.clone());

    let summary = gemini.ask(&mut session)
        .map_err(|e| format!("API error: {}", e))?
        .get_text("");

    Ok(SummarizeResponse {
        summary,
        subtitles: transcript,
        video_name,
    })
}

fn build_static_response(content_type: &str, content: &[u8]) -> Vec<u8> {
    build_response("200 OK", content_type, content)
}

fn build_response(status: &str, content_type: &str, content: &[u8]) -> Vec<u8> {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        status,
        content_type,
        content.len()
    )
        .into_bytes()
        .into_iter()
        .chain(content.iter().cloned())
        .collect()
}

fn write_error_response(stream: &mut TcpStream, status: &str, msg: &str) -> io::Result<()> {
    let response = build_response(status, "text/plain", msg.as_bytes());
    stream.write_all(&response)?;
    stream.flush()
}

fn parse_headers(data: &[u8]) -> Option<(&[u8], usize)> {
    let mut headers_end = 0;
    if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
        headers_end = pos + 4;
    } else {
        return None;
    }

    if headers_end > 0 {
        Some((&data[..headers_end], headers_end))
    } else {
        None
    }
}

fn get_content_length(headers: &[u8]) -> Option<usize> {
    let headers_str = std::str::from_utf8(headers).ok()?;
    for line in headers_str.lines() {
        if line.to_ascii_lowercase().starts_with("content-length:") {
            return line.split(':').nth(1)?.trim().parse().ok();
        }
    }
    None
}

fn read_body(
    buffer: &[u8],
    body_start: usize,
    content_length: usize,
    stream: &mut TcpStream,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::with_capacity(content_length);
    let initial_body_data = &buffer[body_start..];

    if initial_body_data.len() >= content_length {
        body.extend_from_slice(&initial_body_data[..content_length]);
        return Ok(body);
    }

    body.extend_from_slice(initial_body_data);
    let remaining_bytes = content_length - initial_body_data.len();

    if remaining_bytes > 0 {
        let mut rest_of_body = vec![0; remaining_bytes];
        stream.read_exact(&mut rest_of_body)?;
        body.extend_from_slice(&rest_of_body);
    }

    Ok(body)
}