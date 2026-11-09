//! Fish Audio TTS voice output for whisp.
//!
//! Backend for the "Voix" settings section: speak arbitrary text through the
//! Fish Audio TTS API (`POST /v1/tts`, key from the same store as STT) and
//! play it locally. Speed/pitch are NOT exposed: the Fish TTS v1 API has no
//! such parameters (only `reference_id` voice selection), so the UI offers
//! voice selection alone rather than fake sliders.
//!
//! Playback: decoded bytes are written to a temp file and played with the
//! first available player (`mpv` → `ffplay` → `mpg123`).
//! Agent voice reply (`agent_voice` config): when on, Agent dictations are read
//! back after the agent request is queued (see `maybe_speak_agent`).

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
        &["mpg123", "-q"],
    ];
    let mut last_err = "no audio player found (install mpv, ffplay or mpg123)".to_string();
    for argv in attempts {
        if have_bin(argv[0]) {
            let mut args: Vec<&str> = argv[1..].to_vec();
            args.push(&path);
            match std::process::Command::new(argv[0])
                .args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    // Timeout 30 s : un player qui hang ne bloque jamais.
                    let start = std::time::Instant::now();
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) if status.success() => {
                                let _ = std::fs::remove_file(&tmp);
                                return Ok(());
                            }
                            Ok(Some(status)) => {
                                last_err = format!("{}: exit {status}", argv[0]);
                                break;
                            }
                            Ok(None) if start.elapsed() > std::time::Duration::from_secs(30) => {
                                let _ = child.kill();
                                let _ = child.wait();
                                last_err = format!("{}: timeout lecture", argv[0]);
                                break;
                            }
                            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                            Err(e) => {
                                last_err = format!("{}: {e}", argv[0]);
                                break;
                            }
                        }
                    }
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

/// Agent voice reply enabled?
pub fn agent_voice_enabled() -> bool {
    crate::config::load().agent_voice.unwrap_or(false)
}

/// Fire-and-forget Agent read-back: speaks `text` when `agent_voice` is on.
/// Never fails the dictation: errors are swallowed.
pub fn maybe_speak_agent(text: &str) {
    if !agent_voice_enabled() || text.trim().is_empty() {
        return;
    }
    let text = text.to_string();
    let voice = configured_voice();
    std::thread::spawn(move || {
        let _ = speak(&text, voice.as_deref());
    });
}

/// Mode voice : répond à l'utilisateur en vocal. Pipeline : question STT →
/// LLM chat (provider configuré, défaut Ollama local) → Fish TTS lit la réponse.
/// Fire-and-forget : la transcription est déjà collée, la réponse arrive après.
/// Sans voix Fish configurée ou sans LLM joignable : silence + event d'erreur.
pub fn maybe_speak_voice_reply(question: &str, emit: impl Fn(bool) + Send + 'static) {
    let q = question.trim().to_string();
    if q.is_empty() {
        return;
    }
    std::thread::Builder::new()
        .name("whisp-voice-reply".to_string())
        .spawn(move || {
            emit(true);
            let provider = crate::config::load()
                .chat_provider
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "ollama".to_string());
            let msgs = vec![crate::llm::ChatMessage {
                role: "user".to_string(),
                content: q,
            }];
            let reply = match crate::llm::chat(&provider, &msgs) {
                Ok(r) if !r.trim().is_empty() => r,
                Ok(_) => return,
                Err(e) => {
                    eprintln!("[whisp] voice reply LLM: {e}");
                    return;
                }
            };
            // 2. Lecture vocale (Fish TTS, voix configurée ou défaut).
            let voice = configured_voice();
            if voice.is_none() {
                eprintln!("[whisp] voice reply: aucune voix Fish configurée (Settings → Voix)");
            }
            let _ = speak(&reply, voice.as_deref());
            emit(false);
        })
        .ok();
}

/// Tue la lecture TTS en cours (mpv/ffplay/mpg123). Appelé par Échap : la réponse
/// vocale s'arrête net, pas de son qui traîne après la fermeture.
pub fn kill_playback() {
    // Ciblé uniquement : le pattern matche nos fichiers /tmp/whisp-tts-*.mp3.
    // Pas de `pkill -x mpv` : ça tuerait le lecteur vidéo de l'utilisateur.
    // Deadline : appelé par Échap, ne doit jamais figer le cancel.
    let _ = crate::run_timed(
        &["pkill", "-f", "whisp-tts-.*\\.(mp3|wav)"],
        std::time::Duration::from_secs(5),
    );
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Speak `text` through Fish TTS. Returns immediately; playback happens in
/// the background. Errors are emitted as `whisp://tts` events (never blocks
/// the UI — the old blocking version froze the app when the player hung).
#[tauri::command]
pub fn speak_text(
    app: tauri::AppHandle,
    text: String,
    voice: Option<String>,
) -> Result<(), String> {
    let v = voice
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(configured_voice);
    if v.is_none() {
        return Err("Aucune voix configurée : collez un reference_id Fish Audio.".to_string());
    }
    std::thread::Builder::new()
        .name("whisp-tts".to_string())
        .spawn(move || {
            use tauri::Emitter;
            let _ = app.emit("whisp://tts", serde_json::json!({ "state": "playing" }));
            let r = speak(&text, v.as_deref());
            match r {
                Ok(()) => {
                    let _ = app.emit("whisp://tts", serde_json::json!({ "state": "done" }));
                }
                Err(e) => {
                    let _ = app.emit(
                        "whisp://tts",
                        serde_json::json!({ "state": "error", "message": e }),
                    );
                }
            }
        })
        .map_err(|e| format!("spawn tts: {e}"))?;
    Ok(())
}
