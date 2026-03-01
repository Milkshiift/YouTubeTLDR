use miniserde::{json, Deserialize};
use std::error::Error;

#[derive(Deserialize)]
struct PlayerDataResponse {
    captions: Option<Captions>,
    #[serde(rename = "videoDetails")]
    video_details: Option<VideoDetails>,
}

#[derive(Deserialize)]
struct VideoDetails {
    title: String,
}

#[derive(Deserialize)]
struct Captions {
    #[serde(rename = "playerCaptionsTracklistRenderer")]
    player_captions_tracklist_renderer: Option<PlayerCaptionsTracklistRenderer>,
}

#[derive(Deserialize)]
struct PlayerCaptionsTracklistRenderer {
    #[serde(rename = "captionTracks")]
    caption_tracks: Vec<CaptionTrack>,
}

#[derive(Deserialize)]
struct CaptionTrack {
    #[serde(rename = "baseUrl")]
    base_url: String,
    #[serde(rename = "languageCode")]
    language_code: String,
}

#[derive(Deserialize)]
struct JsonCaptionResponse {
    events: Vec<JsonCaptionEvent>,
}

#[derive(Deserialize)]
struct JsonCaptionEvent {
    segs: Option<Vec<CaptionSegment>>,
}

#[derive(Deserialize)]
struct CaptionSegment {
    utf8: String,
}

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36";

struct PlayerConfig {
    client_version: String,
    signature_timestamp: u64,
    api_key: String,
}

pub fn get_video_data(video_url: &str, language: &str) -> Result<(String, String), Box<dyn Error>> {
    let video_id = extract_video_id(video_url)
        .ok_or_else(|| format!("Invalid YouTube URL: {video_url}"))?;

    let config = fetch_player_config(video_id)?;

    let request_body = format!(
        r#"{{
            "context": {{
                "client": {{
                    "clientName": "WEB",
                    "clientVersion": "{client_version}"
                }}
            }},
            "videoId": "{video_id}",
            "playbackContext": {{
                "contentPlaybackContext": {{
                    "signatureTimestamp": {sts}
                }}
            }}
        }}"#,
        client_version = config.client_version,
        sts = config.signature_timestamp,
    );

    let api_url = format!(
        "https://www.youtube.com/youtubei/v1/player?prettyPrint=false&key={}",
        config.api_key
    );

    let player_response = minreq::post(api_url)
        .with_header("User-Agent", USER_AGENT)
        .with_header("Referer", "https://www.youtube.com/")
        .with_body(request_body)
        .send()?;

    let player_data: PlayerDataResponse = json::from_slice(player_response.as_bytes())?;

    let video_title = player_data
        .video_details
        .ok_or("Video details not found")?
        .title;

    let tracks = player_data
        .captions
        .and_then(|c| c.player_captions_tracklist_renderer)
        .map(|r| r.caption_tracks)
        .ok_or_else(|| format!("No captions found for video: {video_id}"))?;

    let track = select_best_track(&tracks, language)?;

    let url = format!("{}&fmt=json3", track.base_url.replace("\\u0026", "&"));
    let caption_response: JsonCaptionResponse = json::from_slice(minreq::get(url).send()?.as_bytes())?;

    let transcript = process_json_captions(caption_response.events);

    Ok((transcript, video_title))
}

fn fetch_player_config(video_id: &str) -> Result<PlayerConfig, Box<dyn Error>> {
    let page_url = format!("https://www.youtube.com/watch?v={video_id}");
    let page_response = minreq::get(&page_url)
        .with_header("User-Agent", USER_AGENT)
        .send()?;
    let page_html = page_response.as_str()?;

    let js_path = extract_json_string_value(page_html, "jsUrl")
        .ok_or("Could not find jsUrl in video page")?;

    let client_version = extract_json_string_value(page_html, "clientVersion")
        .ok_or("Could not find clientVersion")?
        .to_string();

    let api_key = extract_json_string_value(page_html, "INNERTUBE_API_KEY")
        .ok_or("Could not find INNERTUBE_API_KEY")?
        .to_string();

    let js_url = if js_path.starts_with("http") {
        js_path.to_string()
    } else {
        format!("https://www.youtube.com{js_path}")
    };

    let js_response = minreq::get(&js_url)
        .with_header("User-Agent", USER_AGENT)
        .send()?;

    let signature_timestamp = extract_signature_timestamp(js_response.as_str()?)
        .ok_or("Could not find signatureTimestamp in JS player")?;

    Ok(PlayerConfig {
        client_version,
        signature_timestamp,
        api_key,
    })
}

fn extract_json_string_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":\"", key);
    let mut start = 0;
    while let Some(pos) = text[start..].find(&search) {
        let value_start = start + pos + search.len();
        if let Some(end_offset) = text[value_start..].find('"') {
            return Some(&text[value_start..value_start + end_offset]);
        }
        start = value_start;
    }
    None
}

fn extract_signature_timestamp(js_code: &str) -> Option<u64> {
    for needle in &["signatureTimestamp:", "sts:"] {
        let mut search_from = 0;
        while let Some(pos) = js_code[search_from..].find(needle) {
            let abs_pos = search_from + pos + needle.len();

            let digits: String = js_code[abs_pos..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if !digits.is_empty() {
                if let Ok(val) = digits.parse::<u64>() {
                    return Some(val);
                }
            }
            search_from = abs_pos;
        }
    }
    None
}

fn extract_video_id(url: &str) -> Option<&str> {
    const PATTERNS: &[&str] = &["v=", "/embed/", "/live/", "/v/", "/shorts/", "youtu.be/"];

    for pattern in PATTERNS {
        if let Some(pos) = url.find(pattern) {
            let start = pos + pattern.len();
            return url.get(start..start + 11);
        }
    }
    None
}

fn select_best_track<'a>(tracks: &'a [CaptionTrack], language: &str) -> Result<&'a CaptionTrack, Box<dyn Error>> {
    tracks
        .iter()
        .filter(|t| t.language_code == language)
        .min_by_key(|t| {
            if !t.base_url.contains("kind=asr") { 0 }
            else if t.base_url.contains("variant=punctuated") { 1 }
            else { 2 }
        })
        .ok_or_else(|| {
            let available: Vec<_> = tracks.iter().map(|t| &t.language_code).collect();
            format!("No captions for '{language}'. Available: {available:?}").into()
        })
}

fn process_json_captions(events: Vec<JsonCaptionEvent>) -> String {
    let mut result = String::with_capacity(events.len() * 50);

    for event in events {
        if let Some(segs) = event.segs {
            for seg in segs {
                let text = seg.utf8.trim();
                if !text.is_empty() {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push_str(text);
                }
            }
        }
    }

    result
}