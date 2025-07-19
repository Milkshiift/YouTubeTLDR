use regex::Regex;
use serde::Deserialize;
use std::error::Error;
use std::sync::OnceLock;
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

pub fn get_youtube_transcript(video_url: &str, language: &str) -> Result<Vec<TranscriptEntry>, Box<dyn Error>> {
    println!("[1/4] Fetching video page to find API key...");
    let video_id = extract_video_id(video_url)?;
    let res = minreq::get(format!("https://youtu.be/{}", video_id)).send()?;
    let html = res.as_str()?;

    static API_KEY_RE: OnceLock<Regex> = OnceLock::new();
    let api_key = API_KEY_RE
        .get_or_init(|| Regex::new(r#""INNERTUBE_API_KEY":"([^"]+)""#).unwrap())
        .captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
        .ok_or("INNERTUBE_API_KEY not found")?;

    println!("      Found API Key: ...{}", &api_key[api_key.len().saturating_sub(6)..]);

    println!("[2/4] Fetching player data...");
    let player_url = format!("https://www.youtube.com/youtubei/v1/player?key={}", api_key);

    let player_data_response = minreq::post(&player_url)
        .with_json(&serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20210721.00.00"
                }
            },
            "videoId": video_id
        }))?
        .send()?
        .json::<PlayerDataResponse>()?;

    println!("[3/4] Finding caption track for language '{}'...", language);
    let track = player_data_response
        .captions
        .as_ref()
        .and_then(|c| c.player_captions_tracklist_renderer.as_ref())
        .and_then(|r| r.caption_tracks.iter().find(|t| t.language_code == language))
        .ok_or_else(|| format!("No captions found for language: {}", language))?;

    println!("      Found transcript URL.");

    println!("[4/4] Fetching and parsing XML transcript...");
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

pub struct MergeConfig {
    pub paragraph_pause_threshold_secs: f32,
    pub remove_annotations: bool,
}

static ANNOTATION_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn merge_transcript(entries: &[TranscriptEntry], config: &MergeConfig) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let annotation_regex = ANNOTATION_REGEX.get_or_init(|| Regex::new(r"\[[^\]]*\]|\([^)]*\)").unwrap());
    
    let cleaned_entries: Vec<_> = entries.iter()
        .filter_map(|entry| {
            let cleaned = if config.remove_annotations {
                annotation_regex
                    .replace_all(&entry.caption, "")
                    .trim()
                    .to_string()
            } else {
                entry.caption.trim().to_string()
            };

            if cleaned.is_empty() {
                None
            } else {
                Some((entry.start_time, entry.end_time, cleaned))
            }
        })
        .collect();

    if cleaned_entries.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut current_line = cleaned_entries[0].2.clone();

    let is_first_entry_speech = !annotation_regex.replace_all(&cleaned_entries[0].2, "").trim().is_empty();
    let mut last_speech_end_time = if is_first_entry_speech { cleaned_entries[0].1 } else { 0.0 };

    for i in 1..cleaned_entries.len() {
        let (_, prev_end, _) = cleaned_entries[i - 1];
        let (current_start, current_end, current_text) = &cleaned_entries[i];

        let is_speech = !annotation_regex.replace_all(current_text, "").trim().is_empty();
        let pause_duration = if is_speech && last_speech_end_time > 0.0 {
            current_start - last_speech_end_time
        } else {
            0.0
        };

        if is_speech && pause_duration >= config.paragraph_pause_threshold_secs {
            result.push_str(&current_line);
            result.push_str("\n\n");
            current_line = current_text.clone();
        } else {
            let max_overlap = current_line.len().min(current_text.len());
            let overlap = (1..=max_overlap)
                .rev()
                .find(|&len| current_line.ends_with(&current_text[..len]))
                .unwrap_or(0);

            // The ONLY time we perform a smart merge is for true fragmentation:
            // 1. Timestamps must overlap.
            // 2. Text must have a PARTIAL overlap (not full, not zero).
            if *current_start < prev_end && overlap > 0 && overlap < current_text.len() {
                // This is a true fragment, like "Hello wor" + "world".
                // Append the non-overlapping part.
                current_line.push_str(&current_text[overlap..]);
            } else {
                // All other cases are treated as new, distinct words/phrases:
                // - Timestamps don't overlap.
                // - Timestamps overlap but text has zero overlap (e.g., "then." + "All right").
                // - Timestamps overlap but text is a full match (e.g., "Go!" + "Go!").
                current_line.push(' ');
                current_line.push_str(current_text);
            }
        }

        if is_speech {
            last_speech_end_time = last_speech_end_time.max(*current_end);
        }
    }

    result.push_str(&current_line);
    result
}

fn extract_video_id(url: &str) -> Result<String, Box<dyn Error>> {
    static YOUTUBE_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = YOUTUBE_REGEX.get_or_init(|| {
        Regex::new(
            r#"^(?:(?:https?:)?//)?(?:(?:www|m)\.)?(?:youtube\.com|youtu\.be)(?:/(?:[\w-]+\?v=|embed/|v/|shorts/)?|/)([\w-]{11})(?:\S+)?$"#,
        )
            .expect("Invalid regex")
    });

    re.captures(url)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| format!("Invalid YouTube URL: {}", url).into())
}