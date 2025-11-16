mod subtitle;
mod gemini;

use crate::subtitle::get_video_data;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, Read, Write, BufReader, BufRead};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::thread;
use std::time::Duration;
use flume::{bounded, Receiver};
use std::sync::Arc;

#[derive(Deserialize)]
struct SummarizeRequest {
    url: String,
    api_key: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    language: Option<String>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
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
    addr: SocketAddr,
}

struct ServerConfig {
    addr: String,
    num_workers: usize,
    read_timeout: Duration,
    write_timeout: Duration,
    max_body_size: usize,
}

impl ServerConfig {
    fn from_env() -> Self {
        let ip = env::var("TLDR_IP").unwrap_or_else(|_| "0.0.0.0".into());
        let port = env::var("TLDR_PORT").unwrap_or_else(|_| "8000".into());

        Self {
            addr: format!("{}:{}", ip, port),
            num_workers: env::var("TLDR_WORKERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            read_timeout: Duration::from_secs(15),
            write_timeout: Duration::from_secs(15),
            max_body_size: 10 * 1024 * 1024,
        }
    }
}

struct StaticResource {
    content: &'static [u8],
    content_type: &'static str,
}

macro_rules! static_resource {
    ($name:ident, $path:expr, $content_type:expr) => {
        static $name: StaticResource = StaticResource {
            content: include_bytes!(concat!("../static/", $path, ".gz")),
            content_type: $content_type,
        };
    };
}

static_resource!(HTML_RESOURCE, "index.html", "text/html; charset=utf-8");
static_resource!(CSS_RESOURCE, "style.css", "text/css; charset=utf-8");
static_resource!(JS_RESOURCE, "script.js", "application/javascript; charset=utf-8");

fn main() -> io::Result<()> {
    let config = Arc::new(ServerConfig::from_env());

    let listener = TcpListener::bind(&config.addr)?;
    listener.set_nonblocking(false)?; // Better performance

    println!("✅ Server started at http://{}", config.addr);
    println!("✅ Spawning {} worker threads", config.num_workers);

    let (sender, receiver) = bounded(100);

    for id in 0..config.num_workers {
        let receiver = receiver.clone();
        let config = Arc::clone(&config);
        thread::spawn(move || worker(id, receiver, config));
    }

    println!("▶️ Ready to accept requests");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let addr = match stream.peer_addr() {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("❌ Failed to get peer address: {}", e);
                        continue;
                    }
                };

                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(config.read_timeout));
                let _ = stream.set_write_timeout(Some(config.write_timeout));

                let work_item = WorkItem { stream, addr };

                if sender.try_send(work_item).is_err() {
                    eprintln!("⚠️ Queue full, rejecting connection from {}", addr);
                }
            }
            Err(e) => {
                eprintln!("❌ Accept failed: {}", e);
            }
        }
    }
    Ok(())
}

fn worker(id: usize, receiver: Receiver<WorkItem>, config: Arc<ServerConfig>) {
    println!("   Worker {} started", id);

    let mut buffer = Vec::with_capacity(4096);

    while let Ok(mut work_item) = receiver.recv() {
        buffer.clear();

        if let Err(e) = handle_request(&mut work_item.stream, &config, &mut buffer) {
            eprintln!("❌ Worker {} error handling {}: {}", id, work_item.addr, e);
            let _ = write_error_response(&mut work_item.stream, "500 Internal Server Error", &e.to_string());
        }
    }

    println!("   Worker {} shutting down", id);
}

fn handle_request(stream: &mut TcpStream, config: &ServerConfig, buffer: &mut Vec<u8>) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(8192, stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    if request_line.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty request"));
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid request line"));
    }

    let method = parts[0];
    let path = parts[1];

    match method {
        "GET" => handle_get(path, stream),
        "POST" if path == "/api/summarize" => {
            // Read headers
            let mut headers = Vec::new();
            let mut content_length = None;

            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line)?;

                if bytes_read == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF"));
                }

                if line == "\r\n" || line == "\n" {
                    break;
                }

                if line.to_lowercase().starts_with("content-length:") {
                    if let Some(value) = line.split(':').nth(1) {
                        content_length = value.trim().parse().ok();
                    }
                }

                headers.push(line);

                if headers.len() > 100 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Too many headers"));
                }
            }

            let content_length = content_length
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing Content-Length"))?;

            if content_length > config.max_body_size {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Request body too large"));
            }

            // Read body
            buffer.clear();
            buffer.resize(content_length, 0);
            reader.read_exact(buffer)?;

            // Process
            let req: SummarizeRequest = serde_json::from_slice(buffer)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid JSON: {}", e)))?;

            let response_payload = perform_summary_work(req)
                .map_err(|e| io::Error::other(format!("Processing error: {}", e)))?;

            let response_body = serde_json::to_vec(&response_payload)
                .map_err(|e| io::Error::other(format!("JSON serialization error: {}", e)))?;

            write_response(stream, "200 OK", "application/json", &response_body)
        }
        _ => write_error_response(stream, "405 Method Not Allowed", "Method Not Allowed"),
    }
}

fn handle_get(path: &str, stream: &mut TcpStream) -> io::Result<()> {
    let resource = match path {
        "/" | "/index.html" => Some(&HTML_RESOURCE),
        "/style.css" => Some(&CSS_RESOURCE),
        "/script.js" => Some(&JS_RESOURCE),
        _ => None,
    };

    match resource {
        Some(res) => write_static_response(stream, res),
        None => write_error_response(stream, "404 Not Found", "Not Found"),
    }
}

fn write_static_response(stream: &mut TcpStream, resource: &StaticResource) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: {}\r\n\
         Content-Encoding: gzip\r\n\
         Content-Length: {}\r\n\
         Cache-Control: public, max-age=3600\r\n\
         Connection: close\r\n\r\n",
        resource.content_type,
        resource.content.len()
    );

    stream.write_all(response.as_bytes())?;
    stream.write_all(resource.content)?;
    stream.flush()
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, content: &[u8]) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        status, content_type, content.len()
    );

    stream.write_all(response.as_bytes())?;
    stream.write_all(content)?;
    stream.flush()
}

fn write_error_response(stream: &mut TcpStream, status: &str, msg: &str) -> io::Result<()> {
    write_response(stream, status, "text/plain; charset=utf-8", msg.as_bytes())
}

fn perform_summary_work(req: SummarizeRequest) -> Result<SummarizeResponse, String> {
    if req.dry_run {
        let test_md = include_str!("./markdown_test.md");
        return Ok(SummarizeResponse {
            summary: test_md.to_string(),
            subtitles: test_md.to_string(),
            video_name: "Dry Run".to_string(),
        });
    }

    let language = req.language.as_deref().unwrap_or("en");
    let (transcript, video_name) = get_video_data(&req.url, language)
        .map_err(|e| format!("Transcript error: {}", e))?;

    if req.transcript_only {
        return Ok(SummarizeResponse {
            summary: transcript.clone(),
            subtitles: transcript,
            video_name,
        });
    }

    let api_key = req.api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or("Missing Gemini API key. Get one here: https://aistudio.google.com/app/apikey")?;

    let model = req.model
        .as_deref()
        .filter(|m| !m.is_empty())
        .ok_or("Missing model name")?;

    let system_prompt = req.system_prompt
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or("Missing system prompt")?;

    let summary = gemini::summarize(api_key, model, system_prompt, &transcript)
        .map_err(|e| format!("API error: {}", e))?;

    Ok(SummarizeResponse {
        summary,
        subtitles: transcript,
        video_name,
    })
}