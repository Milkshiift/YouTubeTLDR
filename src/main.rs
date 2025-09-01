mod subtitle;
mod gemini;

use crate::subtitle::get_video_data;
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    language: Option<String>,
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
    ($name:ident, $path:expr) => {
        static $name: &[u8] = include_bytes!(concat!("../static/", $path, ".gz"));
    };
}

static_response!(HTML_RESPONSE, "index.html");
static_response!(CSS_RESPONSE, "style.css");
static_response!(JS_RESPONSE, "script.js");

const READ_WRITE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HEADER_SIZE: usize = 8 * 1024; // 8 KB
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB

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
    // Prevent slow clients from holding connections open indefinitely.
    stream.set_read_timeout(Some(READ_WRITE_TIMEOUT))?;
    stream.set_write_timeout(Some(READ_WRITE_TIMEOUT))?;

    let mut stream_clone = stream.try_clone()?;

    let work_item = WorkItem { stream };

    match sender.try_send(work_item) {
        Ok(()) => Ok(()),
        Err(crossbeam_channel::TrySendError::Full(_)) => {
            write_error_response(&mut stream_clone, "503 Service Unavailable", "Server is busy, please try again later.")
        }
        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
            write_error_response(&mut stream_clone, "500 Internal Server Error", "Worker pool has been disconnected.")
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
                    let _ = write_error_response(&mut work_item.stream, "500 Internal Server Error", &e.to_string());
                }
            }
            Err(_) => {
                println!("   Worker {} shutting down", id);
                break;
            }
        }
    }
}

fn handle_request(stream: &mut TcpStream) -> io::Result<()> {
    let (headers, body_start_index) = read_headers_from_stream(stream)?;
    let request_data = &headers[..body_start_index];
    let initial_body = &headers[body_start_index..];

    let mut lines = request_data.split(|&b| b == b'\n').filter(|l| !l.is_empty());
    let request_line = lines.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Empty request"))?;

    if request_line.starts_with(b"GET ") {
        handle_get(request_line, stream)
    } else if request_line.starts_with(b"POST /api/summarize") {
        let content_length = get_content_length(request_data)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Content-Length header is required for POST"))?;

        // Don't allocate any memory if body is too big to prevent OOM
        if content_length > MAX_BODY_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Request body too large"));
        }

        let body = read_body(initial_body, content_length, stream)?;

        let req: SummarizeRequest = serde_json::from_slice(&body)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("JSON deserialization error: {}", e)))?;

        let response_payload = perform_summary_work(req)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Processing error: {}", e)))?;

        let response_body = serde_json::to_string(&response_payload)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON serialization error: {}", e)))?;

        write_response(stream, "200 OK", "application/json", response_body.as_bytes())
    } else {
        write_error_response(stream, "404 Not Found", "Not Found")
    }
}

fn handle_get(request_line: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    let path = request_line.split(|&b| b == b' ').nth(1).unwrap_or(b"/");
    match path {
        b"/" | b"/index.html" => write_static_response(stream, "text/html", HTML_RESPONSE),
        b"/style.css" => write_static_response(stream, "text/css", CSS_RESPONSE),
        b"/script.js" => write_static_response(stream, "application/javascript", JS_RESPONSE),
        _ => write_error_response(stream, "404 Not Found", "Not Found"),
    }
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

    let (transcript, video_name) = get_video_data(&req.url, &req.language.unwrap_or("en".to_string()))
        .map_err(|e| format!("Transcript error: {}", e))?;

    if req.transcript_only {
        return Ok(SummarizeResponse {
            summary: transcript.clone(),
            subtitles: transcript,
            video_name,
        });
    }

    let api_key = req.api_key.as_deref().filter(|k| !k.is_empty()).ok_or("Missing Gemini API key. Get one here: https://aistudio.google.com/app/apikey")?;
    let model = req.model.as_deref().filter(|m| !m.is_empty()).ok_or("Missing model name")?;
    let system_prompt = req.system_prompt.as_deref().filter(|p| !p.is_empty()).ok_or("Missing system prompt")?;

    let summary = gemini::summarize(api_key, model, system_prompt, &transcript)
        .map_err(|e| format!("API error: {}", e))?;

    Ok(SummarizeResponse {
        summary,
        subtitles: transcript,
        video_name,
    })
}

fn read_headers_from_stream(stream: &mut TcpStream) -> io::Result<(Vec<u8>, usize)> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0; 256];
    loop {
        let bytes_read = stream.read(&mut chunk)?;
        if bytes_read == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection closed while reading headers"));
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);

        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let body_start_index = pos + 4;
            return Ok((buffer, body_start_index));
        }

        if buffer.len() > MAX_HEADER_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Headers too large"));
        }
    }
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, content: &[u8]) -> io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        content.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(content)?;
    stream.flush()
}

fn write_static_response(stream: &mut TcpStream, content_type: &str, content: &[u8]) -> io::Result<()> {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        content_type,
        content.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(content)?;
    stream.flush()
}

fn write_error_response(stream: &mut TcpStream, status: &str, msg: &str) -> io::Result<()> {
    write_response(stream, status, "text/plain; charset=utf-8", msg.as_bytes())
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
    initial_data: &[u8],
    content_length: usize,
    stream: &mut TcpStream,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::with_capacity(content_length);
    body.extend_from_slice(initial_data);

    let remaining_bytes = content_length.saturating_sub(initial_data.len());

    if remaining_bytes > 0 {
        let mut remaining_body_reader = stream.take(remaining_bytes as u64); // Ensure we don't read more than content_length
        remaining_body_reader.read_to_end(&mut body)?;
    }

    Ok(body)
}