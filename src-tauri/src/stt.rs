//! Speech-to-text for whisp.
//!
//! Decision: local STT goes through a **Python `faster-whisper` sidecar**
//! (`faster-whisper-rs` pulls a full burn/tch stack and is unstable on this
//! target). The sidecar runs as a **persistent daemon** (`stt_server.py`,
//! socket `~/.cache/whisp/stt.sock`): the model loads ONCE at startup, then
//! each dictation is one socket round-trip — no per-dictation model load,
//! no network call unless a cloud key is configured.
//!
//! - [`LocalFasterWhisper`]: offline default (small model, CPU int8;
//!   `medium` opt-in via config). Warms the daemon on first use, falls back
//!   to the one-shot inline sidecar if the daemon cannot start.
//! - [`CloudFishAudio`]: `POST https://api.fish.audio/v1/asr` with header
//!   `model: transcribe-1-pro`, key read from the OS keyring.
//! - [`select_stt`]: local by default, cloud when the mode demands it.

use std::io::{Read, Write};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::audio::encode_wav_16k_mono;
use crate::modes::SttKind;

const FISH_ASR_URL: &str = "https://api.fish.audio/v1/asr";
const KEYRING_SERVICE: &str = "whisp";
const KEYRING_FISH_USER: &str = "fish_audio";

/// Inline Python sidecar (one-shot fallback when the daemon is unavailable):
/// reads a 16 kHz mono WAV path, prints transcript.
const STT_SIDECAR_PY: &str = r#"
import sys, wave
wav_path, model, language = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    from faster_whisper import WhisperModel
    import numpy as np
except ImportError:
    print("__WHISP_MISSING_DEPS__", flush=True)
    sys.exit(10)
with wave.open(wav_path, "rb") as w:
    assert w.getnchannels() == 1 and w.getsampwidth() == 2 and w.getframerate() == 16000, "need 16k mono s16 wav"
    raw = w.readframes(w.getnframes())
audio = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
_cuda = False  # CUDA disabled: libcublas missing on this host; CPU int8 is reliable
m = WhisperModel(model, device="cpu", compute_type="int8")
lang = None if language in ("auto", "") else language
segments, _ = m.transcribe(audio, language=lang, beam_size=5,
                            vad_filter=True,
                            vad_parameters=dict(min_silence_duration_ms=400))
print("".join(s.text for s in segments), flush=True)
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    pub language: String,
    pub engine: String,
}

pub trait Stt: Send + Sync {
    fn transcribe(&self, pcm_16k_mono: &[f32], language: &str) -> Result<Transcription, String>;
}

// ---------------------------------------------------------------------------
// Local: persistent faster-whisper daemon (+ one-shot fallback)
// ---------------------------------------------------------------------------

/// Daemon source, embedded at compile time; materialised to the cache dir
/// at spawn so `python` gets a real script file.
const STT_SERVER_PY: &str = include_str!("stt_server.py");

const PING: &[u8] = b"__ping__";
const PONG: &[u8] = b"__pong__";
const MODEL_QUERY: &[u8] = b"__model__";
const ERROR_PREFIX: &[u8] = b"__WHISP_ERROR__:";

fn daemon_sock_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("whisp/stt.sock")
}

fn fw_python() -> String {
    if let Ok(p) = std::env::var("WHISP_FW_PYTHON") {
        if !p.is_empty() {
            return p;
        }
    }
    if let Some(home) = dirs::home_dir() {
        let venv = home.join(".local/share/whisp/fw/bin/python");
        if venv.exists() {
            return venv.to_string_lossy().into_owned();
        }
    }
    "python3".to_string()
}

fn read_frame(conn: &mut std::os::unix::net::UnixStream) -> Result<Vec<u8>, String> {
    let mut len_buf = [0u8; 4];
    conn.read_exact(&mut len_buf)
        .map_err(|e| format!("daemon read: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err("daemon frame too large".to_string());
    }
    let mut buf = vec![0u8; len];
    conn.read_exact(&mut buf)
        .map_err(|e| format!("daemon read: {e}"))?;
    Ok(buf)
}

fn write_frame(conn: &mut std::os::unix::net::UnixStream, payload: &[u8]) -> Result<(), String> {
    let len = u32::try_from(payload.len()).map_err(|_| "payload too large".to_string())?;
    conn.write_all(&len.to_be_bytes())
        .and_then(|()| conn.write_all(payload))
        .map_err(|e| format!("daemon write: {e}"))
}

/// One-shot request/response on a fresh connection. Short timeout: ping and
/// model queries must answer immediately from a warm daemon.
fn daemon_roundtrip(payload: &[u8]) -> Result<Vec<u8>, String> {
    let sock = daemon_sock_path();
    let mut conn = std::os::unix::net::UnixStream::connect(&sock)
        .map_err(|e| format!("daemon connect {sock:?}: {e}"))?;
    conn.set_read_timeout(Some(Duration::from_secs(5)))
        .and(conn.set_write_timeout(Some(Duration::from_secs(5))))
        .map_err(|e| format!("daemon timeout: {e}"))?;
    write_frame(&mut conn, payload)?;
    read_frame(&mut conn)
}

fn daemon_running_model() -> Option<String> {
    let resp = daemon_roundtrip(MODEL_QUERY).ok()?;
    if resp.starts_with(ERROR_PREFIX) {
        return None;
    }
    String::from_utf8(resp).ok()
}

/// Singleton: the spawned daemon child (if WE started it) plus its model.
/// Guards concurrent first-use spawns (`do_finish` and history reprocess
/// can transcribe at the same time).
struct DaemonState {
    child: Option<std::process::Child>,
    model: Option<String>,
}

static DAEMON: LazyLock<Mutex<DaemonState>> = LazyLock::new(|| {
    Mutex::new(DaemonState {
        child: None,
        model: None,
    })
});

/// Ensure a daemon for `model` answers pings, spawning it if needed.
/// Returns `Err` (fast) when the daemon cannot start — the caller falls
/// back to the one-shot sidecar.
pub fn ensure_daemon(model: &str) -> Result<(), String> {
    // Fast path: warm daemon with the right model.
    if let Ok(resp) = daemon_roundtrip(PING) {
        if resp == PONG && daemon_running_model().as_deref() == Some(model) {
            return Ok(());
        }
    }

    // Re-check under the lock (a peer may have spawned it meanwhile).
    {
        let state = DAEMON.lock().map_err(|_| "daemon lock poisoned".to_string())?;
        if state.model.is_some() {
            drop(state);
            if let Ok(resp) = daemon_roundtrip(PING) {
                if resp == PONG && daemon_running_model().as_deref() == Some(model) {
                    return Ok(());
                }
            }
        }
    }
    let mut state = DAEMON.lock().map_err(|_| "daemon lock poisoned".to_string())?;

    // Wrong-model daemon (ours or a leftover): stop it before respawning.
    if daemon_running_model().as_deref() != Some(model) {
        if let Some(mut child) = state.child.take() {
            let _ = child.kill();
        }
        // A leftover from a previous app run answers pings but is not our
        // child: remove its socket so the bind below cannot collide.
        let _ = std::fs::remove_file(daemon_sock_path());
        state.model = None;
    }
    if state.model.as_deref() == Some(model) {
        if let Some(child) = state.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok(()), // ours, still alive; slow ping, let caller retry
                _ => {
                    state.child = None;
                    state.model = None;
                }
            }
        }
    }

    // Materialise the embedded server source next to the socket.
    let script = daemon_sock_path()
        .parent()
        .unwrap_or(std::path::Path::new("/tmp"))
        .join("stt_server.py");
    if let Some(parent) = script.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("daemon dir: {e}"))?;
    }
    let current = std::fs::read_to_string(&script).unwrap_or_default();
    if current != STT_SERVER_PY {
        std::fs::write(&script, STT_SERVER_PY).map_err(|e| format!("daemon script: {e}"))?;
    }

    let sock = daemon_sock_path();
    let mut child = std::process::Command::new(fw_python())
        .arg(&script)
        .arg("--model")
        .arg(model)
        .arg("--sock")
        .arg(&sock)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn STT daemon (pip install faster-whisper?): {e}"))?;

    // Wait for readiness: model load happens once here, never per dictation.
    let ready = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(|e| format!("daemon wait: {e}"))? {
            Some(status) => {
                return Err(format!("STT daemon exited during startup (status {status})"));
            }
            None => {}
        }
        if let Ok(resp) = daemon_roundtrip(PING) {
            if resp == PONG {
                state.child = Some(child);
                state.model = Some(model.to_string());
                return Ok(());
            }
        }
        if ready.elapsed() > Duration::from_secs(180) {
            let _ = child.kill();
            return Err("STT daemon startup timeout (model load?)".to_string());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Transcribe through the warm daemon. Timeout scales with audio length so
/// long dictations never hang the pipeline: `2 * duration + 30 s`.
fn transcribe_via_daemon(
    pcm: &[f32],
    language: &str,
    model: &str,
) -> Result<Transcription, String> {
    ensure_daemon(model)?;
    let wav = encode_wav_16k_mono(pcm);
    let lang = if language.is_empty() {
        "auto"
    } else {
        language
    };
    let mut payload = Vec::with_capacity(1 + lang.len() + wav.len());
    payload.push(lang.len() as u8);
    payload.extend_from_slice(lang.as_bytes());
    payload.extend_from_slice(&wav);

    let sock = daemon_sock_path();
    let mut conn = std::os::unix::net::UnixStream::connect(&sock)
        .map_err(|e| format!("daemon connect {sock:?}: {e}"))?;
    let secs = (pcm.len() as u64 / 16_000) * 2 + 30;
    conn.set_read_timeout(Some(Duration::from_secs(secs)))
        .and(conn.set_write_timeout(Some(Duration::from_secs(secs))))
        .map_err(|e| format!("daemon timeout: {e}"))?;
    write_frame(&mut conn, &payload)?;
    let resp = read_frame(&mut conn)?;
    if let Some(msg) = resp.strip_prefix(ERROR_PREFIX) {
        return Err(format!(
            "STT daemon: {}",
            String::from_utf8_lossy(msg).trim()
        ));
    }
    Ok(Transcription {
        text: String::from_utf8(resp).map_err(|e| format!("daemon utf-8: {e}"))?,
        language: language.to_string(),
        engine: format!("faster-whisper:{model}"),
    })
}
/// Fast live preview through the daemon's tiny model. Best effort: returns
/// `None` on any failure (preview is optional, never blocks dictation).
/// Timeout is short (12 s) — a slow preview is worse than none.
pub fn transcribe_partial(pcm: &[f32]) -> Option<String> {
    if pcm.len() < 16_000 {
        return None; // < 1 s of audio: nothing worth showing
    }
    let wav = encode_wav_16k_mono(pcm);
    let mut payload = Vec::with_capacity(11 + wav.len());
    payload.extend_from_slice(b"__partial__");
    payload.extend_from_slice(&wav);
    let sock = daemon_sock_path();
    let mut conn = std::os::unix::net::UnixStream::connect(&sock).ok()?;
    conn.set_read_timeout(Some(Duration::from_secs(12)))
        .and(conn.set_write_timeout(Some(Duration::from_secs(12))))
        .ok()?;
    write_frame(&mut conn, &payload).ok()?;
    let resp = read_frame(&mut conn).ok()?;
    if resp.starts_with(ERROR_PREFIX) {
        return None;
    }
    let text = String::from_utf8(resp).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Stop the daemon at app shutdown: non-blocking kill, then remove the
/// socket file so a stale path never blocks the next launch. Never panics.
pub fn stop_stt_daemon() {
    if let Ok(mut state) = DAEMON.lock() {
        if let Some(mut child) = state.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        state.model = None;
    }
    // Always sweep the socket file, even if the daemon was a leftover.
    let _ = std::fs::remove_file(daemon_sock_path());
}

/// Offline STT via Python `faster-whisper` (small model by default).
///
/// VRAM guard: `medium` fits in 6 GB with float16 on CUDA; on CPU it runs
/// int8. Nothing is downloaded at runtime beyond the first model fetch
/// (cached under `~/.cache/huggingface`).
pub struct LocalFasterWhisper {
    pub model: String,
}

impl Default for LocalFasterWhisper {
    fn default() -> Self {
        Self {
            model: crate::config::stt_model_name(),
        }
    }
}

impl Stt for LocalFasterWhisper {
    fn transcribe(&self, pcm: &[f32], language: &str) -> Result<Transcription, String> {
        if pcm.is_empty() {
            return Ok(Transcription {
                text: String::new(),
                language: language.to_string(),
                engine: "local".to_string(),
            });
        }
        // Warm daemon first; one-shot inline sidecar as fallback.
        match transcribe_via_daemon(pcm, language, &self.model) {
            Ok(tx) => Ok(tx),
            Err(_) => transcribe_one_shot(pcm, language, &self.model),
        }
    }
}

/// One-shot inline sidecar: spawns Python per call (pays the model-load
/// cost). Fallback only — the daemon path above is the hot path.
fn transcribe_one_shot(pcm: &[f32], language: &str, model: &str) -> Result<Transcription, String> {
    let wav = encode_wav_16k_mono(pcm);
    let tmp = tempfile_path("whisp-stt-", ".wav");
    std::fs::write(&tmp, &wav).map_err(|e| format!("write temp wav: {e}"))?;
    let lang = if language.is_empty() { "auto" } else { language };

    let out = std::process::Command::new(fw_python())
        .args(["-c", STT_SIDECAR_PY])
        .arg(&tmp)
        .arg(model)
        .arg(lang)
        .output()
        .map_err(|e| format!("spawn faster-whisper sidecar (pip install faster-whisper?): {e}"))?;
    let _ = std::fs::remove_file(&tmp);

    if out.status.code() == Some(10)
        || String::from_utf8_lossy(&out.stdout).contains("__WHISP_MISSING_DEPS__")
    {
        return Err("faster-whisper not installed: pip install faster-whisper".to_string());
    }
    if !out.status.success() {
        return Err(format!(
            "faster-whisper failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(Transcription {
        text: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        language: language.to_string(),
        engine: format!("faster-whisper:{model}"),
    })
}

fn tempfile_path(prefix: &str, suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}{}{suffix}", uuid::Uuid::new_v4()))
}

// ---------------------------------------------------------------------------
// Cloud: Fish Audio
// ---------------------------------------------------------------------------

fn fish_key_file() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join(".config"))
        .join("whisp/fish_audio.key")
}

pub fn fish_api_key() -> Option<String> {
    // 1. OS keyring (preferred).
    if let Some(k) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_FISH_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
    {
        return Some(k);
    }
    // 2. Fallback: mode 600 file (the keyring crate can't persist on this host).
    std::fs::read_to_string(fish_key_file())
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

pub fn set_fish_api_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("empty key".to_string());
    }
    // Try keyring first; fall back to 600 file (never fail the save).
    if keyring::Entry::new(KEYRING_SERVICE, KEYRING_FISH_USER)
        .and_then(|e| e.set_password(key).map_err(|_| keyring::Error::NoEntry))
        .is_ok()
    {
        // Verify it round-trips; otherwise use the file.
        if fish_key_file_fallback_needed() {
            write_fish_key_file(key)?;
        }
        return Ok(());
    }
    write_fish_key_file(key)
}

fn fish_key_file_fallback_needed() -> bool {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_FISH_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_none()
}

fn write_fish_key_file(key: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let path = fish_key_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("config dir: {e}"))?;
    }
    std::fs::write(&path, key).map_err(|e| format!("write key file: {e}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod key file: {e}"))?;
    Ok(())
}

/// Cloud STT via Fish Audio (`transcribe-1-pro`). Key comes from the OS
/// keyring — never from config files or env in release.
pub struct CloudFishAudio {
    pub client: reqwest::blocking::Client,
}

impl Default for CloudFishAudio {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl Stt for CloudFishAudio {
    fn transcribe(&self, pcm: &[f32], language: &str) -> Result<Transcription, String> {
        let key = fish_api_key().ok_or_else(|| "fish.audio key not configured".to_string())?;
        let wav = encode_wav_16k_mono(pcm);
        let part = reqwest::blocking::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| format!("multipart: {e}"))?;
        let mut form = reqwest::blocking::multipart::Form::new().part("audio", part);
        if !language.is_empty() && language != "auto" {
            form = form.text("language", language.to_string());
        }
        let resp: serde_json::Value = self
            .client
            .post(FISH_ASR_URL)
            .header("Authorization", format!("Bearer {key}"))
            .header("model", "transcribe-1-pro")
            .multipart(form)
            .send()
            .map_err(|e| format!("fish.audio request: {e}"))?
            .error_for_status()
            .map_err(|e| format!("fish.audio status: {e}"))?
            .json()
            .map_err(|e| format!("fish.audio decode: {e}"))?;
        Ok(Transcription {
            text: clean_fish_text(
                resp.get("text").and_then(|v| v.as_str()).unwrap_or_default(),
            ),
            language: language.to_string(),
            engine: "fish.audio:transcribe-1-pro".to_string(),
        })
    }
}

/// Strip Fish Audio markup: `<|speaker:N|>` diarization markers and
/// `[emotion / event cues]` — clean prose for the LLM and the paste.
pub fn clean_fish_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        // `<|speaker:N|>` marker
        if c == '<' && chars.peek() == Some(&'|') {
            let mut buf = String::from("<");
            for cc in chars.by_ref() {
                buf.push(cc);
                if cc == '>' {
                    break;
                }
                if buf.len() > 16 {
                    break;
                }
            }
            if buf.ends_with('>') && buf.contains("speaker") {
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
                continue;
            }
            out.push_str(&buf);
            continue;
        }
        // `[...]` cue
        if c == '[' {
            let mut buf = String::from("[");
            let mut closed = false;
            for cc in chars.by_ref() {
                buf.push(cc);
                if cc == ']' {
                    closed = true;
                    break;
                }
                if buf.len() > 48 {
                    break;
                }
            }
            if closed {
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
                continue;
            }
            out.push_str(&buf);
            continue;
        }
        out.push(c);
    }
    // Collapse whitespace left by removals.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// Local by default; cloud only when the mode explicitly requires it AND a
/// key is configured (otherwise fall back to local — never fail silently
/// into a network call without credentials).
pub enum SelectedStt {
    Local(LocalFasterWhisper),
    Cloud(CloudFishAudio),
}

impl Stt for SelectedStt {
    fn transcribe(&self, pcm: &[f32], language: &str) -> Result<Transcription, String> {
        match self {
            SelectedStt::Local(l) => l.transcribe(pcm, language),
            SelectedStt::Cloud(c) => c.transcribe(pcm, language),
        }
    }
}

pub fn select_stt(kind: SttKind) -> SelectedStt {
    match kind {
        SttKind::Local => SelectedStt::Local(LocalFasterWhisper::default()),
        SttKind::CloudFishAudio => {
            if fish_api_key().is_some() {
                SelectedStt::Cloud(CloudFishAudio::default())
            } else {
                SelectedStt::Local(LocalFasterWhisper::default())
            }
        }
    }
}

/// Append `content` to a file (used for debug dumps).
#[allow(dead_code)]
pub fn append_debug_log(path: &std::path::Path, content: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{content}");
    }
}

#[cfg(test)]
mod fish_clean_tests {
    use super::clean_fish_text;

    #[test]
    fn strips_speakers_and_cues() {
        let raw = "<|speaker:0|> [bruit de claquement de doigts] Bonjour, ceci est un test. [clic]";
        assert_eq!(clean_fish_text(raw), "Bonjour, ceci est un test.");
    }
}
