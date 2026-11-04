//! Fish Audio TTS voice output for whisp.
//!
//! Backend for the "Voix" settings section: speak arbitrary text through the
//! Fish Audio TTS API (`POST /v1/tts`, key from the same store as STT) and
//! play it locally. Speed/pitch are NOT exposed: the Fish TTS v1 API has no
//! such parameters (only `reference_id` voice selection), so the UI offers
//! voice selection alone rather than fake sliders.
//!
//! Playback: decoded bytes are written to a temp file and played with the
//! first available player (`mpv` → `ffplay` → `paplay`/`aplay` for wav).
//! STA voice reply (`sta_voice` config): when on, STA dictations are read
//! back after the agent request is queued (see `maybe_speak_sta`).

use std::io::Write;
use std::time::Duration;

const FISH_TTS_URL: &str = "https://api.fish.audio/v1/tts";
/// Fish TTS model; `speech-1.5` is the current general model.
const FISH_TTS_MODEL: &str = "speech-1.5";

fn http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Raw TTS: text -> audio bytes (mp3). Key from the shared Fish store.
pub fn tts_bytes(text: &str, voice: Option<&str>) -> Result<Vec<u8>, String> {
    let key =
        crate::stt::fish_api_key().ok_or_else(|| "fish.audio key not configured".to_string())?;
    let text = text.trim();
    if text.is_empty() {
        return Err("empty text".to_string());
    }
    let mut body = serde_json::json!({
        "text": text,
        "format": "mp3",
        "mp3_bitrate": 128,
        "normalize": true,
        "model": FISH_TTS_MODEL,
    });
    if let Some(v) = voice.map(str::trim).filter(|v| !v.is_empty()) {
        body["reference_id"] = serde_json::Value::String(v.to_string());
    }
    let resp = http()
        .post(FISH_TTS_URL)
        .header("Authorization", format!("Bearer {key}"))
        .json(&body)
        .send()
        .map_err(|e| format!("fish.audio tts request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("fish.audio tts status: {e}"))?;
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("fish.audio tts read: {e}"))
}

/// Play mp3 bytes with the first available player. Blocking.
fn play_mp3(bytes: &[u8]) -> Result<(), String> {
    let mut tmp = std::env::temp_dir().join(format!("whisp-tts-{}.mp3", uuid::Uuid::new_v4()));
    let _ = tmp.set_extension("mp3");
    std::fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("tts temp file: {e}"))?;
    let path = tmp.to_string_lossy().to_string();
    let attempts: &[&[&str]] = &[
        &["mpv", "--no-video", "--really-quiet"],
        &["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet"],
    ];
    let mut last_err = "no player found (mpv/ffplay)".to_string();
    for argv in attempts {
        if have_bin(argv[0]) {
            let mut args: Vec<&str> = argv[1..].to_vec();
            args.push(&path);
            match std::process::Command::new(argv[0])
                .args(&args)
                .output()
            {
                Ok(out) if out.status.success() => {
                    let _ = std::fs::remove_file(&tmp);
                    return Ok(());
                }
                Ok(out) => {
                    last_err = format!(
                        "{}: {}",
                        argv[0],
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                }
                Err(e) => last_err = format!("{}: {e}", argv[0]),
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last_err)
}

fn have_bin(name: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Speak `text` with the configured Fish voice (blocking ~seconds).
pub fn speak(text: &str, voice: Option<&str>) -> Result<(), String> {
    let bytes = tts_bytes(text, voice)?;
    play_mp3(&bytes)
}

/// Configured Fish voice reference id, if any.
pub fn configured_voice() -> Option<String> {
    crate::config::load()
        .fish_voice
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// STA voice reply enabled?
pub fn sta_voice_enabled() -> bool {
    crate::config::load().sta_voice.unwrap_or(false)
}

/// Fire-and-forget STA read-back: speaks `text` when `sta_voice` is on.
/// Never fails the dictation: errors are swallowed.
pub fn maybe_speak_sta(text: &str) {
    if !sta_voice_enabled() || text.trim().is_empty() {
        return;
    }
    let text = text.to_string();
    let voice = configured_voice();
    std::thread::spawn(move || {
        let _ = speak(&text, voice.as_deref());
    });
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Speak `text` through Fish TTS (blocking; the frontend disables Tester).
#[tauri::command]
pub fn speak_text(text: String, voice: Option<String>) -> Result<(), String> {
    let v = voice
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(configured_voice);
    speak(&text, v.as_deref())
}
