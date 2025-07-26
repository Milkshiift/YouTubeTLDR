use serde::Deserialize;
use std::error::Error;
use html_escape::decode_html_entities;

#[derive(Debug, Deserialize)]
pub struct TranscriptEntry {
    pub caption: String,
    pub start_time: f32,
    pub end_time: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerDataResponse {
    captions: Option<Captions>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Captions {
    player_captions_tracklist_renderer: Option<PlayerCaptionsTracklistRenderer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerCaptionsTracklistRenderer {
    caption_tracks: Vec<CaptionTrack>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptionTrack {
    base_url: String,
    language_code: String,
}

#[derive(Debug, Deserialize)]
struct Transcript {
    #[serde(rename = "text", default)]
    text: Vec<TextEntry>,
}

#[derive(Debug, Deserialize)]
struct TextEntry {
    #[serde(rename = "@start")]
    start: f32,
    #[serde(rename = "@dur")]
    dur: f32,
    #[serde(rename = "$value")]
    content: String,
}

pub fn get_video_data(video_url: &str, language: &str) -> Result<(Vec<TranscriptEntry>, String), Box<dyn Error>> {
    let video_id = extract_video_id(video_url)?;
    let res = minreq::get(format!("https://youtu.be/{video_id}"))
        .with_header("Referer", "https://www.youtube.com/")
        .with_header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.3")
        .send()?;
    let html = res.as_str()?;
    
    let transcript = get_youtube_transcript(html, &video_id, language)?;
    let video_name = get_video_name(html).unwrap_or("Unknown Title".to_string());

    Ok((transcript, video_name))
}

fn get_youtube_transcript(html: &str, video_id: &str, language: &str) -> Result<Vec<TranscriptEntry>, Box<dyn Error>> {
    let api_key = html.split(r#""INNERTUBE_API_KEY":""#)
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8");
    
    let player_url = format!("https://www.youtube.com/youtubei/v1/player?key={api_key}");

    let player_data_response = minreq::post(&player_url)
        .with_header("Referer", "https://www.youtube.com/")
        .with_header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.3")
        .with_json(&serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20250626.01.00"
                }
            },
            "videoId": video_id
        }))?
        .send()?
        .json::<PlayerDataResponse>()?;
    
    let track = player_data_response
        .captions
        .as_ref()
        .and_then(|c| c.player_captions_tracklist_renderer.as_ref())
        .and_then(|r| r.caption_tracks.iter().find(|t| t.language_code == language))
        .ok_or_else(|| format!("No captions found for language: {language}"))?;
    
    let res = minreq::get(&track.base_url).send()?; 
    let xml = res.as_bytes();
    let parsed_xml: Transcript = quick_xml::de::from_reader(xml)?;

    let transcript = parsed_xml.text
        .into_iter()
        .map(|entry| {
            let decoded_caption = decode_html_entities(&entry.content).into_owned();
            TranscriptEntry {
                caption: decoded_caption,
                start_time: entry.start,
                end_time: entry.start + entry.dur,
            }
        })
        .collect();

    Ok(transcript)
}

fn get_video_name(html: &str) -> Option<String> {
    html.split_once(r#"<meta name="title" content=""#)?
        .1
        .split_once(r#"">"#)?
        .0
        .to_string()
        .into()
}

pub struct MergeConfig {
    pub paragraph_pause_threshold_secs: f32,
    pub remove_annotations: bool,
}

fn remove_annotations(text: &str) -> String {
    let mut cleaned = String::new();
    let mut depth = 0;
    for c in text.chars() {
        if c == '[' || c == '(' {
            depth += 1;
        } else if (c == ']' || c == ')') && depth > 0 {
            depth -= 1;
        } else if depth == 0 {
            cleaned.push(c);
        }
    }
    cleaned
}

pub fn merge_transcript(entries: &[TranscriptEntry], config: &MergeConfig) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(entries.len() * 40);
    let mut current_line = String::with_capacity(256);

    let mut last_speech_end_time = entries[0].end_time;

    for (i, entry) in entries.iter().enumerate() {
        let cleaned_caption = if config.remove_annotations {
            remove_annotations(&entry.caption)
        } else {
            entry.caption.clone()
        };
        let cleaned_caption = cleaned_caption.trim();

        if cleaned_caption.is_empty() {
            continue;
        }

        if i > 0 {
            let pause_duration = entry.start_time - last_speech_end_time;
            if pause_duration >= config.paragraph_pause_threshold_secs {
                result.push_str(&current_line);
                result.push_str("\n\n");
                current_line.clear();
            } else {
                current_line.push(' ');
            }
        }

        current_line.push_str(cleaned_caption);
        last_speech_end_time = last_speech_end_time.max(entry.end_time);
    }

    result.push_str(&current_line);
    result
}

fn extract_video_id(url: &str) -> Result<String, Box<dyn Error>> {
    if let Some(id) = url.split("v=").nth(1) {
        return Ok(id.chars().take(11).collect());
    }
    if let Some(id) = url.split("/embed/").nth(1) {
        return Ok(id.chars().take(11).collect());
    }
    if let Some(id) = url.split("/v/").nth(1) {
        return Ok(id.chars().take(11).collect());
    }
    if let Some(id) = url.split("/shorts/").nth(1) {
        return Ok(id.chars().take(11).collect());
    }
    if let Some(id) = url.split("youtu.be/").nth(1) {
        return Ok(id.chars().take(11).collect());
    }
    Err(format!("Invalid YouTube URL: {url}").into())
}