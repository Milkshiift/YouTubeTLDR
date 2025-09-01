use serde::{Deserialize};
use std::error::Error;


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


pub fn get_video_data(video_url: &str, language: &str) -> Result<(String, String), Box<dyn Error>> {
    let video_id = extract_video_id(video_url)
        .ok_or_else(|| format!("Invalid or unsupported YouTube URL: {}", video_url))?;

    let (transcript, video_name) = get_transcript_and_title(&video_id, language)?;

    Ok((transcript, video_name))
}


fn get_transcript_and_title(video_id: &str, language: &str) -> Result<(String, String), Box<dyn Error>> {
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
        .ok_or("Video details not found in API response. Server IP likely blocked by YouTube.")?
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

    let transcript = process_json_captions(json_response.events);

    Ok((transcript, video_title))
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
        .ok_or_else(|| {
            let available_languages: Vec<String> = tracks
                .iter()
                .map(|track| track.language_code.clone())
                .collect();
            let unique_languages: Vec<String> = available_languages.into_iter().collect::<std::collections::HashSet<String>>().into_iter().collect();
            let joined_languages = unique_languages.join(", ");

            format!(
                "No suitable captions found for language '{}'. Available languages: [{}]",
                language,
                joined_languages
            ).into()
        })
}

fn process_json_captions(events: Vec<JsonCaptionEvent>) -> String {
    events
        .into_iter()
        .filter_map(|event| {
            match event {
                JsonCaptionEvent::CaptionEvent { segs: Some(segs), .. } => {
                    let caption_text: String = segs
                        .iter()
                        .map(|s| s.utf8.trim())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<&str>>()
                        .join(" ");

                    if caption_text.is_empty() {
                        None
                    } else {
                        Some(caption_text)
                    }
                }
                _ => None,
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}