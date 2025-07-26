use serde::{Deserialize};
use std::error::Error;
use html_escape::decode_html_entities;


#[derive(Debug, Deserialize)]
pub struct TranscriptEntry {
    pub caption: String,
    pub start_time: f32,
    pub end_time: f32,
}

pub struct MergeConfig {
    pub paragraph_pause_threshold_secs: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerDataResponse {
    captions: Option<Captions>,
    video_details: Option<VideoDetails>,
}

#[derive(Deserialize)]
struct VideoDetails {
    title: String,
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

#[derive(Deserialize)]
struct JsonCaptionResponse {
    events: Vec<JsonCaptionEvent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonCaptionEvent {
    CaptionEvent {
        #[serde(rename = "tStartMs")]
        start_ms: u64,
        #[serde(rename = "dDurationMs")]
        duration_ms: u64,
        segs: Option<Vec<CaptionSegment>>,
    },
    MetadataEvent {
        #[serde(flatten)]
        _extra: serde_json::Value,
    },
}

#[derive(Deserialize)]
struct CaptionSegment {
    utf8: String,
}


const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const YOUTUBE_REFERER: &str = "https://www.youtube.com/";
const YOUTUBE_BASE_URL: &str = "https://www.youtube.com";


pub fn get_video_data(video_url: &str, language: &str) -> Result<(Vec<TranscriptEntry>, String), Box<dyn Error>> {
    let video_id = extract_video_id(video_url)
        .ok_or_else(|| format!("Invalid or unsupported YouTube URL: {}", video_url))?;
    
    let (transcript, video_name) = get_transcript_and_title(&video_id, language)?;
    
    let decoded_video_name = decode_html_entities(&video_name).into_owned();

    Ok((transcript, decoded_video_name))
}

pub fn merge_transcript(entries: &[TranscriptEntry], config: &MergeConfig) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(entries.len() * 40);
    let mut current_line = String::with_capacity(256);
    let mut last_speech_end_time = entries[0].start_time;

    for entry in entries {
        let cleaned_caption = entry.caption.trim();
        if cleaned_caption.is_empty() {
            continue;
        }

        let pause_duration = entry.start_time - last_speech_end_time;
        if !current_line.is_empty() {
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


fn get_transcript_and_title(video_id: &str, language: &str) -> Result<(Vec<TranscriptEntry>, String), Box<dyn Error>> {
    let api_key = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

    let player_url = format!("{}/youtubei/v1/player?key={}", YOUTUBE_BASE_URL, api_key);

    let player_data_response = minreq::post(player_url)
        .with_header("User-Agent", USER_AGENT)
        .with_header("Referer", YOUTUBE_REFERER)
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
    
    let video_title = player_data_response
        .video_details
        .ok_or("Video details not found in API response")?
        .title;
    
    let tracks = player_data_response
        .captions
        .and_then(|c| c.player_captions_tracklist_renderer)
        .map(|r| r.caption_tracks)
        .ok_or_else(|| format!("No captions found for video ID: {}", video_id))?;

    let track = select_best_track(&tracks, language)?;
    let captions_url = format_captions_url(&track.base_url);

    let caption_res = minreq::get(captions_url).send()?;
    let caption_json_str = caption_res.as_str()?;

    let json_response: JsonCaptionResponse = serde_json::from_str(caption_json_str)
        .map_err(|e| format!("Failed to parse captions JSON: {}\nResponse: {}", e, caption_json_str))?;

    let transcript_entries = process_json_captions(json_response.events);

    Ok((transcript_entries, video_title))
}

fn extract_video_id(url: &str) -> Option<String> {
    let extract_id = |s: &str| s.chars().take(11).collect::<String>();

    url.split_once("v=")
        .or_else(|| url.split_once("/embed/"))
        .or_else(|| url.split_once("/v/"))
        .or_else(|| url.split_once("/shorts/"))
        .or_else(|| url.split_once("youtu.be/"))
        .map(|(_, after)| extract_id(after))
}

fn format_captions_url(base_url: &str) -> String {
    // The base URL from the API may contain unicode-escaped ampersands.
    format!("{}&fmt=json3", base_url.replace("\\u0026", "&"))
}

fn select_best_track<'a>(tracks: &'a [CaptionTrack], language: &str) -> Result<&'a CaptionTrack, Box<dyn Error>> {
    let mut manual_track = None;
    let mut punctuated_asr_track = None;
    let mut plain_asr_track = None;

    for track in tracks {
        if track.language_code == language {
            let url = &track.base_url;

            if !url.contains("kind=asr") {
                manual_track = Some(track);
                break;
            }

            if url.contains("variant=punctuated") {
                if punctuated_asr_track.is_none() {
                    punctuated_asr_track = Some(track);
                }
            }

            else {
                if plain_asr_track.is_none() {
                    plain_asr_track = Some(track);
                }
            }
        }
    }

    manual_track
        .or(punctuated_asr_track)
        .or(plain_asr_track)
        .ok_or_else(|| format!("No suitable captions found for language '{}'", language).into())
}

fn process_json_captions(events: Vec<JsonCaptionEvent>) -> Vec<TranscriptEntry> {
    events
        .into_iter()
        .filter_map(|event| {
            let (start_ms, duration_ms, segs) = match event {
                JsonCaptionEvent::CaptionEvent { start_ms, duration_ms, segs: Some(segs) } => {
                    (start_ms, duration_ms, segs)
                }
                _ => return None,
            };

            let caption_text: String = segs
                .iter()
                .map(|s| s.utf8.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<&str>>()
                .join(" ");

            if caption_text.is_empty() {
                return None;
            }

            let decoded_caption = decode_html_entities(&caption_text).into_owned();

            Some(TranscriptEntry {
                caption: decoded_caption,
                start_time: start_ms as f32 / 1000.0,
                end_time: (start_ms + duration_ms) as f32 / 1000.0,
            })
        })
        .collect()
}