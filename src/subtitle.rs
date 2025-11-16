use miniserde::{Deserialize, json};
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

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
const API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

pub fn get_video_data(video_url: &str, language: &str) -> Result<(String, String), Box<dyn Error>> {
    let video_id =
        extract_video_id(video_url).ok_or_else(|| format!("Invalid YouTube URL: {}", video_url))?;

    get_transcript_and_title(video_id, language)
}

fn get_transcript_and_title(
    video_id: &str,
    language: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let request_body = format!(
        r#"{{
            "context": {{
                "client": {{
                    "clientName": "WEB",
                    "clientVersion": "2.20251113.00.00"
                }}
            }},
            "videoId": "{}"
        }}"#,
        video_id
    );

    let player_response = minreq::post(format!(
        "https://www.youtube.com/youtubei/v1/player?prettyPrint=false&key={}",
        API_KEY
    ))
    .with_header("User-Agent", USER_AGENT)
    .with_header("Referer", "https://www.youtube.com/")
    .with_body(request_body)
    .send()?;

    let player_data: PlayerDataResponse = json::from_slice(player_response.as_bytes())?;

    let video_title = player_data
        .video_details
        .ok_or("Video not found or server IP blocked by YouTube")?
        .title;

    let tracks = player_data
        .captions
        .and_then(|c| c.player_captions_tracklist_renderer)
        .map(|r| r.caption_tracks)
        .ok_or_else(|| format!("No captions found for video: {}", video_id))?;

    let track = select_best_track(&tracks, language)?;

    let url = format!("{}&fmt=json3", track.base_url.replace("\\u0026", "&"));
    let caption_response: JsonCaptionResponse =
        json::from_slice(minreq::get(url).send()?.as_bytes())?;
    let transcript = process_json_captions(caption_response.events);

    Ok((transcript, video_title))
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

fn select_best_track<'a>(
    tracks: &'a [CaptionTrack],
    language: &str,
) -> Result<&'a CaptionTrack, Box<dyn Error>> {
    // manual > punctuated ASR > plain ASR
    let mut best = None;
    let mut priority = 999;

    for track in tracks {
        if track.language_code == language {
            let track_priority = if !track.base_url.contains("kind=asr") {
                0 // Manual
            } else if track.base_url.contains("variant=punctuated") {
                1 // Punctuated ASR
            } else {
                2 // Plain ASR
            };

            if track_priority < priority {
                best = Some(track);
                priority = track_priority;
                if priority == 0 {
                    break;
                } // Found manual, stop searching
            }
        }
    }

    best.ok_or_else(|| {
        let available: Vec<_> = tracks.iter().map(|t| &t.language_code).collect();
        format!("No captions for '{}'. Available: {:?}", language, available).into()
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
