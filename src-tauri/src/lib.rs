//! whisp backend: recorder popup + settings, offline-first dictation.
//!
//! Frontend contract (see WhispPopup):
//! - Commands (snake_case): `start_recording`, `stop_recording`,
//!   `cancel_recording`, `cycle_mode`, plus modes/history/settings helpers.
//! - Events: `whisp://state` ({status, message?, mode?}) and `whisp://level`
//!   (f32 0..1).

mod agent_hook;
mod appearance;
mod audio;
mod config;
mod history;
mod hotkeys;
mod island;
mod llm;
mod modes;
mod shortcuts;
mod stt;
mod updates;
mod voice;

use std::sync::Mutex;
use std::time::Duration;


use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use crate::stt::Stt as _;
use crate::llm::Llm as _;

pub struct AppState {
    session: Mutex<SessionState>,
    mode_id: Mutex<String>,
    sta_last: Mutex<Option<String>>,
    /// True while `do_finish` transcription runs (STT+LLM, tens of seconds).
    /// Lets a concurrent `stop` answer `{busy:true}` instead of blocking.
    transcribing: std::sync::atomic::AtomicBool,
}

/// Single-lock session lifecycle: Idle | Starting | Recording(handle).
/// `do_start`/`do_finish`/`do_cancel` all transition under this one lock,
/// so two concurrent toggles can never both spawn a capture.
enum SessionState {
    Idle,
    /// `do_start` holds this from the `spawn_session` call until the handle
    /// is stored; any peer seeing it refuses with "already recording".
    Starting,
    Recording(audio::RecordingHandle),
}

impl AppState {
    fn new() -> Self {
        Self {
            session: Mutex::new(SessionState::Idle),
            mode_id: Mutex::new("message".to_string()),
            sta_last: Mutex::new(None),
            transcribing: std::sync::atomic::AtomicBool::new(false),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct StatePayload {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecStatus {
    Idle,
    Recording,
    Processing,
    Done,
    Error,
}

impl RecStatus {
    fn as_str(self) -> &'static str {
        match self {
            RecStatus::Idle => "idle",
            RecStatus::Recording => "recording",
            RecStatus::Processing => "processing",
            RecStatus::Done => "done",
            RecStatus::Error => "error",
        }
    }
}

fn current_mode_id(state: &State<AppState>) -> String {
    state.mode_id.lock().expect("mode lock poisoned").clone()
}

fn emit_state(app: &tauri::AppHandle, status: RecStatus, message: Option<String>) {
    let mode = app
        .try_state::<AppState>()
        .map(|s: State<AppState>| current_mode_id(&s));
    let _ = app.emit(
        "whisp://state",
        StatePayload {
            status: status.as_str().to_string(),
            message,
            mode,
        },
    );
}
/// Cycle/set the mode then emit the mode change while preserving the current
/// pipeline status: cycling mid-dictation must NOT reset the popup to idle,
/// and cycling during transcription must NOT drop `processing` either.
fn emit_mode_changed(app: &tauri::AppHandle, state: &State<'_, AppState>, next: &modes::Mode) {
    *state.mode_id.lock().expect("mode lock poisoned") = next.id.clone();
    let transcribing = state
        .transcribing
        .load(std::sync::atomic::Ordering::SeqCst);
    let recording = state
        .session
        .lock()
        .map(|g| matches!(*g, SessionState::Recording(_) | SessionState::Starting))
        .unwrap_or(false);
    let st = if transcribing {
        RecStatus::Processing
    } else if recording {
        RecStatus::Recording
    } else {
        RecStatus::Idle
    };
    emit_state(app, st, Some(format!("mode: {}", next.id)));
}

fn show_recorder(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("recorder") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn hide_recorder(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("recorder") {
        let _ = w.hide();
    }
}

// ---------------------------------------------------------------------------
// Recording pipeline
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResult {
    pub transcript_raw: String,
    pub text_final: String,
    pub mode: String,
    pub reused: bool,
}

fn do_start(app: &tauri::AppHandle) -> Result<(), String> {
    // Claim Starting under the single session lock: a concurrent toggle sees
    // Starting|Recording and gets a clean refusal instead of a second capture.
    {
        let state = app.state::<AppState>();
        let mut guard = state.session.lock().expect("session lock poisoned");
        match *guard {
            SessionState::Idle => *guard = SessionState::Starting,
            SessionState::Starting | SessionState::Recording(_) => {
                return Err("already recording".to_string())
            }
        }
    }
    // Capture spawn (up to ~5 s for the device) runs WITHOUT the lock so
    // `status`/`cancel` stay responsive; the state reads Starting meanwhile.
    let spawned = audio::spawn_session(app.clone(), true);
    let state = app.state::<AppState>();
    let mut guard = state.session.lock().expect("session lock poisoned");
    match spawned {
        Ok(session) => {
            *guard = SessionState::Recording(session);
            drop(guard);
            show_recorder(app);
            emit_state(app, RecStatus::Recording, None);
            spawn_partial_thread(app.clone());
            // Warm the STT daemon now so live previews work from the first
            // seconds (and the final transcribe pays no model-load cost).
            let model = config::stt_model_name();
            std::thread::Builder::new()
                .name("whisp-stt-warm".to_string())
                .spawn(move || {
                    // Serialized inside ensure_daemon; concurrent warms queue.
                    let _ = stt::ensure_daemon(&model);
                })
                .ok();
            Ok(())
        }
        Err(e) => {
            *guard = SessionState::Idle;
            Err(e)
        }
    }
}

/// Live preview loop: every 2.5 s, snapshot the ring (non-destructive),
/// transcribe fast via the daemon tiny model, emit `whisp://partial`.
/// Exits when the session disappears (stop/cancel). Best effort, silent.
fn spawn_partial_thread(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("whisp-partial".to_string())
        .spawn(move || {
            use tauri::Emitter;
            // Let ~3 s of audio accumulate before the first preview.
            std::thread::sleep(std::time::Duration::from_millis(3000));
            let mut last_shown = String::new();
            loop {
                // Opt-out: realtime preview disabled in config.
                let enabled = config::realtime_preview();
                if !enabled {
                    return;
                }
                let pcm: Option<Vec<f32>> = app.try_state::<AppState>().and_then(|s| {
                    s.session.lock().ok().and_then(|g| match &*g {
                        SessionState::Recording(h) => Some(h.snapshot()),
                        SessionState::Idle | SessionState::Starting => None,
                    })
                });
                let pcm = match pcm {
                    Some(p) if p.len() >= 32_000 => p,
                    _ => break, // session gone (stop/cancel): exit
                };
                // Sliding window: last 12 s only (tiny is fast, old words stable).
                let window: Vec<f32> = if pcm.len() > 16_000 * 12 {
                    pcm[pcm.len() - 16_000 * 12..].to_vec()
                } else {
                    pcm
                };
                if let Some(text) = stt::transcribe_partial(&window) {
                    // Stability: common prefix + max 8 new words. No dancing text.
                    let stable = stable_prefix(&last_shown, &text);
                    if stable != last_shown {
                        last_shown = stable.clone();
                        let _ = app.emit("whisp://partial", serde_json::json!({ "text": stable }));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(3000));
            }
        })
        .ok();
}

/// Longest word-stable prefix between last shown and new transcript,
/// plus up to 8 trailing words. Empty if nothing in common.
fn stable_prefix(old: &str, new: &str) -> String {
    let old_w: Vec<&str> = old.split_whitespace().collect();
    let new_w: Vec<&str> = new.split_whitespace().collect();
    let mut common = 0;
    while common < old_w.len() && common < new_w.len() && old_w[common] == new_w[common] {
        common += 1;
    }
    let take = (common + 8).min(new_w.len());
    new_w[..take].join(" ")
}

fn do_cancel(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut guard = state.session.lock().expect("session lock poisoned");
        match *guard {
            SessionState::Idle => return Err("not recording".to_string()),
            // Drop the Starting claim or the live handle; the capture thread
            // (if any) exits on its stop channel, the ring is discarded.
            SessionState::Starting | SessionState::Recording(_) => *guard = SessionState::Idle,
        }
    }
    hide_recorder(app);
    emit_state(app, RecStatus::Idle, None);
    Ok(())
}

/// True while `do_finish` runs the STT+LLM pipeline (not just draining).
fn is_transcribing(app: &tauri::AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|s: State<'_, AppState>| {
            s.transcribing.load(std::sync::atomic::Ordering::SeqCst)
        })
        .unwrap_or(false)
}

/// Drain the session, transcribe -> refine -> history -> clipboard/paste.
/// Take is done under the lock, `finish()` (100 ms sleep) outside it.
async fn do_finish(app: tauri::AppHandle, entry_mode: modes::Mode) -> Result<StopResult, String> {
    // Take the handle under the single lock; refus propre if Starting/Idle.
    let handle: audio::RecordingHandle = {
        let state = app.state::<AppState>();
        let mut guard = state.session.lock().expect("session lock poisoned");
        match std::mem::replace(&mut *guard, SessionState::Idle) {
            SessionState::Recording(h) => h,
            SessionState::Starting => {
                *guard = SessionState::Starting;
                return Err("starting, try again".to_string());
            }
            SessionState::Idle => return Err("not recording".to_string()),
        }
    };
    finish_from_handle(app, entry_mode, handle).await
}

/// Shared transcription tail: drain `handle` -> STT+LLM -> history/paste.
/// Used by `do_finish` (Tauri commands) and the socket `stop` (background
/// task after an immediate {started:true} reply). Sets the transcribing
/// flag around the pipeline; errors surface via whisp://state Error.
async fn finish_from_handle(
    app: tauri::AppHandle,
    entry_mode: modes::Mode,
    handle: audio::RecordingHandle,
) -> Result<StopResult, String> {
    // `finish()` sleeps 100 ms for the final callback: outside the lock.
    let pcm: Vec<f32> = handle.finish();

    if pcm.len() < audio::TARGET_SAMPLE_RATE as usize / 10 {
        hide_recorder(&app);
        emit_state(&app, RecStatus::Idle, Some("too short".to_string()));
        return Ok(StopResult {
            transcript_raw: String::new(),
            text_final: String::new(),
            mode: entry_mode.id.clone(),
            reused: false,
        });
    }

    emit_state(&app, RecStatus::Processing, None);
    // Flag the STT+LLM window so a concurrent `stop` answers busy instead of
    // blocking, and `status` reports transcribing. Cleared on every exit.
    if let Some(state) = app.try_state::<AppState>() {
        state
            .transcribing
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    let is_sta = entry_mode.id == "sta";
    let mode_id_done = entry_mode.id.clone();
    let pipeline: Result<(String, String, Vec<u8>), String> =
        tauri::async_runtime::spawn_blocking(move || {
            let stt = stt::select_stt(entry_mode.stt);
            // Global dictation language wins over mode "auto" (avoids
            // sticky-lang surprises: a past English detection forcing French).
            let lang = if entry_mode.language == "auto" {
                config::dictation_language()
            } else {
                entry_mode.language.clone()
            };
            let tx = stt.transcribe(&pcm, &lang)?;
            let llm = llm::select_llm(entry_mode.llm);
            let refined = llm.refine(&tx.text, &entry_mode)?;
            let wav = audio::encode_wav_16k_mono(&pcm);
            let entry =
                history::insert(&entry_mode.id, &tx.text, &refined.text, Some(&wav))?;
            let _ = entry;
            Ok::<(String, String, Vec<u8>), String>((tx.text, refined.text, wav))
        })
        .await
        .map_err(|e| format!("pipeline task: {e}"))?;
    // Pipeline done (ok or err): clear the flag before propagating.
    if let Some(state) = app.try_state::<AppState>() {
        state
            .transcribing
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    let (raw, text_final, _wav): (String, String, Vec<u8>) = pipeline?;
    let sta_reused = if is_sta {
        match launch_agent(&text_final) {
            Ok((reused, full)) => {
                if let Some(prompt) = full {
                    if let Some(state) = app.try_state::<AppState>() {
                        *state.sta_last.lock().expect("sta lock poisoned") = Some(prompt);
                    }
                    // La pill STA apparaît dans l'island (working).
                    island::upsert_sta_pill(
                        &app,
                        format!("sta-{}", config::sta_agent_id()),
                        config::sta_agent_id(),
                        text_final.chars().take(80).collect(),
                    );
                }
                // Réponse vocale STA (Fish TTS) quand `sta_voice` est activé.
                voice::maybe_speak_sta(&text_final);
                reused
            }
            Err(e) => {
                // Spawn failed twice: keep the request in history, toast the
                // reason through the recorder state message, no silent drop.
                emit_state(&app, RecStatus::Error, Some(e));
                return Err("Agent introuvable, vérifie Configuration".to_string());
            }
        }
    } else if config::auto_paste() {
        paste_text(&text_final);
        false
    } else {
        // Collage auto désactivé : texte en presse-papiers + historique uniquement.
        copy_to_clipboard(&text_final);
        false
    };
    hide_recorder(&app);
    emit_state(&app, RecStatus::Done, None);
    // Return to idle shortly after so the popup can reset its UI — but only
    // if no new recording started in the meantime (late Idle must not clobber
    // a fresh Recording state).
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let still_idle = app2
            .try_state::<AppState>()
            .map(|s: State<'_, AppState>| {
                matches!(*s.session.lock().expect("session lock poisoned"), SessionState::Idle)
            })
            .unwrap_or(true);
        if still_idle {
            emit_state(&app2, RecStatus::Idle, None);
        }
    });

    Ok(StopResult {
        transcript_raw: raw,
        text_final,
        // #7: entry mode captured at stop, not the (maybe cycled) current one.
        mode: mode_id_done,
        reused: sta_reused,
    })
}


// ---------------------------------------------------------------------------
// STA (speech-to-agent): one persistent terminal, reused dictation after
// dictation (hybrid smart mode).
//
// - Session check: tmux session `whisp-sta` when tmux exists, else a running
//   terminal titled `whisp-sta` (pgrep).
// - Session live -> the new request is injected (`tmux send-keys`); the
//   existing terminal is focused, never duplicated.
// - No session -> created (foot, fallback kitty/xterm) running the agent.
// - Context joined to the prompt: `Contexte: dossier <cwd>. Demande: <text>`
//   (+ screenshot path when `grim` captured one). User speech is never logged.
// ---------------------------------------------------------------------------

/// Persistent agent session (tmux name + terminal title).
const STA_SESSION: &str = "whisp-sta";

fn have_bin(name: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn tmux_session_active() -> bool {
    have_bin("tmux")
        && std::process::Command::new("tmux")
            .args(["has-session", "-t", STA_SESSION])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// A visible agent terminal is already open (foot/kitty/xterm titled whisp-sta).
fn sta_term_visible() -> bool {
    std::process::Command::new("pgrep")
        .args(["-f", STA_SESSION])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// True when a previous STA dictation left a live session behind.
fn sta_session_active() -> bool {
    tmux_session_active() || sta_term_visible()
}

/// Bring the existing agent terminal forward (Hyprland) instead of opening
/// a second one. Best effort.
fn focus_sta_term() {
    let _ = std::process::Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &format!("title:{STA_SESSION}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Open the visible agent terminal running `shell_cmd` (a `sh -c` body).
/// foot first, then kitty, then xterm. Silent when no terminal exists.
fn spawn_sta_term(shell_cmd: &str) {
    let null = || std::process::Stdio::null();
    if have_bin("foot") {
        let _ = std::process::Command::new("foot")
            .args(["--title", STA_SESSION, "-e", "sh", "-c", shell_cmd])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    } else if have_bin("kitty") {
        let _ = std::process::Command::new("kitty")
            .args(["--title", STA_SESSION, "sh", "-c", shell_cmd])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    } else if have_bin("xterm") {
        let _ = std::process::Command::new("xterm")
            .args(["-title", STA_SESSION, "-e", "sh", "-c", shell_cmd])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    }
}

/// Active project folder: first existing directory among path-like tokens in
/// the focused window's class/title (`hyprctl activewindow -j`); the
/// configured default otherwise. Best effort, never blocks dictation.
fn resolve_sta_cwd() -> String {
    let default = config::sta_cwd_default();
    if !config::sta_context_enabled() {
        return default;
    }
    let text = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let text = match text {
        Some(t) => t,
        None => return default,
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return default,
    };
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut candidates = Vec::new();
    for key in ["title", "class"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            candidates.extend(s.split_whitespace().map(|s| s.to_string()));
        }
    }
    for tok in candidates {
        let clean = tok.trim_matches(|c: char| {
            c == '"' || c == '\'' || c == '(' || c == ')' || c == ',' || c == ';'
        });
        if clean.is_empty() {
            continue;
        }
        let expanded = if let Some(rest) = clean.strip_prefix("~/") {
            format!("{home}/{rest}")
        } else if clean == "~" {
            home.clone()
        } else {
            clean.to_string()
        };
        // Tolerate editor suffixes (`/path/file.rs:12`).
        let mut tried = expanded.clone();
        loop {
            if tried.starts_with('/') {
                let p = std::path::Path::new(&tried);
                if p.is_dir() {
                    return tried;
                }
                if p.is_file() {
                    if let Some(parent) = p.parent().and_then(|x| x.to_str()) {
                        return parent.to_string();
                    }
                }
            }
            let Some(stripped) = tried
                .rsplit_once(':')
                .filter(|(_, line)| line.chars().all(|c| c.is_ascii_digit()))
                .map(|(base, _)| base.to_string())
            else {
                break;
            };
            tried = stripped;
        }
    }
    default
}

/// One lightweight fullscreen capture via `grim`. Returns the image path, or
/// None (context off, no grim, failure). Best effort.
fn capture_sta_screenshot() -> Option<String> {
    if !config::sta_context_enabled() || !have_bin("grim") {
        return None;
    }
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|r| format!("{r}/whisp-sta-screen.png"))
        .unwrap_or_else(|_| "/tmp/whisp-sta-screen.png".to_string());
    let ok = std::process::Command::new("grim")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Some(path)
    } else {
        None
    }
}

/// Agent prompt: `Contexte: dossier <cwd>. [Capture d'écran: <path>.]
/// Demande: <transcript>`. No agent preset takes an image flag today, so the
/// screenshot travels as a path inside the text prompt. Plain transcript when
/// context is off.
fn sta_prompt(request: &str, cwd: &str, shot: Option<&str>) -> String {
    if !config::sta_context_enabled() {
        return request.to_string();
    }
    match shot {
        Some(p) => format!("Contexte: dossier {cwd}. Capture d'écran: {p}. Demande: {request}"),
        None => format!("Contexte: dossier {cwd}. Demande: {request}"),
    }
}

/// Agent invocation for one prompt (single line: `tmux send-keys` types it
/// literally). Presets: `omp -p`, `claude`, `codex exec`, or the custom
/// `sta_command` template with `{prompt}` (shell-quoted).
fn sta_inner_command(agent: &str, template: &str, prompt: &str) -> String {
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let quoted = format!("'{}'", flat.replace('\'', "'\\''"));
    if !template.is_empty() {
        return template.replace("{prompt}", &quoted);
    }
    // Binaire manquant -> fallback omp (évite terminal qui s'ouvre et meurt).
    let bin = match agent {
        "claude" => "claude",
        "codex" => "codex",
        _ => "omp",
    };
    let effective = if have_bin(bin) { agent } else { "omp" };
    match effective {
        "claude" => format!("claude {quoted}"),
        "codex" => format!("codex exec {quoted}"),
        _ => format!("omp -p {quoted}"),
    }
}

/// Run one shell command with a deadline. `Err` on spawn failure, non-zero
/// exit, or timeout — the caller retries once, then surfaces the message.
fn run_timed(argv: &[&str], timeout: Duration) -> Result<(), String> {
  if argv.is_empty() {
    return Err("empty command".to_string());
  }
  let mut child = std::process::Command::new(argv[0])
    .args(&argv[1..])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|e| format!("spawn {}: {e}", argv[0]))?;
  let start = std::time::Instant::now();
  loop {
    match child.try_wait().map_err(|e| format!("wait {}: {e}", argv[0]))? {
      Some(status) if status.success() => return Ok(()),
      Some(status) => return Err(format!("{} exited with {status}", argv[0])),
      None => {
        if start.elapsed() >= timeout {
          let _ = child.kill();
          let _ = child.wait();
          return Err(format!("{} timed out after {}s", argv[0], timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
      }
    }
  }
}

/// Run one STA request through the persistent agent session.
/// Returns `(reused, full_prompt)`: `reused` is true when the request was
/// injected into the live session; `full_prompt` is the context-carrying
/// prompt (None when the transcript was empty). `Err` when the spawn
/// failed twice (retry once) or no agent binary/terminal is available —
/// history keeps the request either way; user speech is never logged.
fn launch_agent(request: &str) -> Result<(bool, Option<String>), String> {
  const SPAWN_TIMEOUT: Duration = Duration::from_secs(120);
  let request = request.trim();
  if request.is_empty() {
    return Ok((false, None));
  }
  let cwd = resolve_sta_cwd();
  let shot = capture_sta_screenshot();
  let prompt = sta_prompt(request, &cwd, shot.as_deref());
  let agent = config::sta_agent_id();
  let template = config::sta_command_template();
  let inner = sta_inner_command(&agent, &template, &prompt);
  // Run from the project folder even if the session shell moved elsewhere.
  let inner = format!("cd '{}' && {inner}", cwd.replace('\'', "'\\''"));

  if have_bin("tmux") {
    let live = tmux_session_active();
    if !live {
      // Fresh persistent session: a plain shell (stays alive between
      // requests, so the next dictation can inject into it). Retry once —
      // the daemon can be mid-restart when the first spawn races it.
      let mut created = run_timed(&["tmux", "new-session", "-d", "-s", STA_SESSION], SPAWN_TIMEOUT);
      if created.is_err() {
        created = run_timed(&["tmux", "new-session", "-d", "-s", STA_SESSION], SPAWN_TIMEOUT);
      }
      if let Err(e) = created {
        return Err(format!("Agent introuvable, vérifie Configuration (tmux: {e})"));
      }
    }
    if tmux_session_active() {
      // Inject the request into the session shell (retry once on failure).
      let mut sent = run_timed(&["tmux", "send-keys", "-t", STA_SESSION, &inner, "Enter"], SPAWN_TIMEOUT);
      if sent.is_err() {
        sent = run_timed(&["tmux", "send-keys", "-t", STA_SESSION, &inner, "Enter"], SPAWN_TIMEOUT);
      }
      if let Err(e) = sent {
        return Err(format!("Agent introuvable, vérifie Configuration (send-keys: {e})"));
      }
      if sta_term_visible() {
        focus_sta_term();
      } else {
        spawn_sta_term(&format!(
          "exec tmux attach-session -t {STA_SESSION} 2>/dev/null || exec tmux new-session -s {STA_SESSION}"
        ));
        // Laisse le terminal apparaître, puis ramène-le devant.
        std::thread::sleep(std::time::Duration::from_millis(600));
        focus_sta_term();
      }
      return Ok((live, Some(prompt)));
    }
    // tmux present but unusable: fall through to a plain terminal.
  }

  // No tmux: no reliable injection, run the request in its own terminal.
  if sta_term_visible() {
    focus_sta_term();
  }
  if !(have_bin("foot") || have_bin("kitty") || have_bin("xterm")) {
    return Err("Agent introuvable, vérifie Configuration (aucun terminal: foot|kitty|xterm)".to_string());
  }
  spawn_sta_term(&format!(
    "{inner}; echo; echo '[whisp-sta done — Enter to close]'; read _"
  ));
  Ok((false, Some(prompt)))
}

/// Clipboard + auto-paste into the previously focused app.
/// Best effort: failures keep text in history (and usually clipboard).
/// Clipboard only (no auto-paste). Used when `auto_paste` is off.
fn copy_to_clipboard(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    std::process::Command::new("wl-copy")
        .arg("--type")
        .arg("text/plain")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            Ok::<_, std::io::Error>(())
        })
        .is_ok()
}

fn paste_text(text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let wl_ok = copy_to_clipboard(text);
    // 2. Paste via Ctrl+V (unicode-safe: accents pass through the clipboard,
    // unlike `ydotool type` which mangles non-ASCII on some layouts).
    // Give the compositor a beat to restore focus after hiding.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let pasted = std::process::Command::new("ydotool")
        .args(["key", "ctrl+v"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !pasted && !wl_ok {
        eprintln!("[whisp] paste failed: ydotool + wl-copy unavailable?");
    }
    // Text stays in history either way (best effort).
}

// ---------------------------------------------------------------------------
// Tauri commands (frontend contract)
// ---------------------------------------------------------------------------

#[tauri::command]
fn start_recording(app: tauri::AppHandle) -> Result<(), String> {
    do_start(&app)
}

#[tauri::command]
async fn stop_recording(app: tauri::AppHandle) -> Result<StopResult, String> {
    let mode_id = app
        .try_state::<AppState>()
        .map(|s: State<'_, AppState>| current_mode_id(&s))
        .unwrap_or_else(|| "message".to_string());
    let mode =
        modes::get_mode(&mode_id).unwrap_or_else(|| modes::builtin_modes().into_iter().next().unwrap());
    do_finish(app, mode).await
}

#[tauri::command]
fn cancel_recording(app: tauri::AppHandle) -> Result<(), String> {
    do_cancel(&app)
}

#[tauri::command]
fn recording_status(app: tauri::AppHandle, state: State<'_, AppState>) -> String {
    let _ = &app;
    if state
        .transcribing
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return RecStatus::Processing.as_str().to_string();
    }
    match *state.session.lock().expect("session lock poisoned") {
        SessionState::Recording(_) | SessionState::Starting => {
            RecStatus::Recording.as_str().to_string()
        }
        SessionState::Idle => RecStatus::Idle.as_str().to_string(),
    }
}


#[tauri::command]
fn cycle_mode(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<modes::Mode, String> {
    let all = modes::list_modes();
    if all.is_empty() {
        return Err("no modes".to_string());
    }
    let cur = current_mode_id(&state);
    let idx = all.iter().position(|m| m.id == cur).unwrap_or(0);
    let next = all[(idx + 1) % all.len()].clone();
    emit_mode_changed(&app, &state, &next);
    Ok(next)
}

#[tauri::command]
fn get_current_mode(state: State<'_, AppState>) -> Result<modes::Mode, String> {
    let id = current_mode_id(&state);
    modes::get_mode(&id).ok_or_else(|| format!("unknown mode '{id}'"))
}

#[tauri::command]
fn set_mode(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<modes::Mode, String> {
    let mode = modes::get_mode(&id).ok_or_else(|| format!("unknown mode '{id}'"))?;
    emit_mode_changed(&app, &state, &mode);
    Ok(mode)
}

#[tauri::command]
fn list_modes() -> Vec<modes::Mode> {
    modes::list_modes()
}

#[tauri::command]
fn upsert_mode(mode: modes::Mode) -> Result<modes::Mode, String> {
    modes::upsert_custom_mode(mode)
}

#[tauri::command]
fn delete_mode(id: String) -> Result<(), String> {
    modes::delete_custom_mode(&id)
}

#[tauri::command]
fn list_history(limit: Option<u32>) -> Result<Vec<history::HistoryEntry>, String> {
    history::list(limit.unwrap_or(50))
}

#[tauri::command]
fn search_history(query: String) -> Result<Vec<history::HistoryEntry>, String> {
    history::search(&query)
}

#[tauri::command]
fn get_history(id: String) -> Result<Option<history::HistoryEntry>, String> {
    history::get(&id)
}

#[tauri::command]
fn delete_history(id: String) -> Result<(), String> {
    history::delete(&id)
}

#[tauri::command]
async fn reprocess_history(
    app: tauri::AppHandle,
    id: String,
    mode_id: Option<String>,
) -> Result<history::HistoryEntry, String> {
    let entry = history::get(&id)?.ok_or_else(|| format!("history entry '{id}' not found"))?;
    let want = mode_id.unwrap_or_else(|| entry.mode.clone());
    let mode = modes::get_mode(&want).ok_or_else(|| format!("unknown mode '{want}'"))?;
    let raw = entry.transcript_raw.clone();
    let updated = tauri::async_runtime::spawn_blocking(move || {
        let llm = llm::select_llm(mode.llm);
        let refined = llm.refine(&raw, &mode)?;
        history::update_final(&id, &refined.text)
    })
    .await
    .map_err(|e| format!("reprocess task: {e}"))??;
    let _ = &app;
    Ok(updated)
}

#[tauri::command]
fn get_config() -> config::AppConfig {
    config::load()
}

#[tauri::command]
fn set_config(
    popup_pos: Option<String>,
    stt_model: Option<String>,
    theme: Option<String>,
    language: Option<String>,
    input_device: Option<String>,
    sta_agent: Option<String>,
    sta_command: Option<String>,
    sta_cwd: Option<String>,
    sta_context: Option<bool>,
    realtime_preview: Option<bool>,
    auto_paste: Option<bool>,
    active_pills: Option<Vec<String>>,
    ollama_url: Option<String>,
    ollama_model: Option<String>,
    chat_provider: Option<String>,
    chat_model: Option<String>,
    chat_base_url: Option<String>,
    fish_voice: Option<String>,
    sta_voice: Option<bool>,
    popup_size: Option<String>,
    card_opacity: Option<f64>,
    popup_mochi: Option<bool>,
) -> Result<(), String> {
    config::set(
        popup_pos,
        stt_model,
        theme,
        language,
        input_device,
        sta_agent,
        sta_command,
        sta_cwd,
        sta_context,
        realtime_preview,
        auto_paste,
        active_pills,
        ollama_url,
        ollama_model,
        chat_provider,
        chat_model,
        chat_base_url,
        fish_voice,
        sta_voice,
        popup_size,
        card_opacity,
        popup_mochi,
    )
}

/// STA session status for the settings card: `{active, agent, cwd,
/// lastPrompt}`. `active` = persistent session alive (tmux `whisp-sta` or a
/// terminal titled `whisp-sta`); `cwd` = folder the next request would run
/// from; `lastPrompt` = full context-carrying prompt of the last STA
/// dictation, or null. Never fails: degrades to inactives on missing state.
#[tauri::command]
fn sta_status(app: tauri::AppHandle) -> serde_json::Value {
    let last: Option<String> = app
        .try_state::<AppState>()
        .and_then(|s: State<'_, AppState>| s.sta_last.lock().ok().and_then(|g| g.clone()));
    serde_json::json!({
        "active": sta_session_active(),
        "agent": config::sta_agent_id(),
        "cwd": resolve_sta_cwd(),
        "lastPrompt": last,
    })
}

/// Open (or focus) the agent terminal: focus `whisp-sta` when visible,
/// otherwise open a terminal with a shell in the default cwd. No agent is
/// launched; errors are returned as strings.
#[tauri::command]
fn sta_open_terminal() -> Result<(), String> {
    if sta_term_visible() {
        focus_sta_term();
        return Ok(());
    }
    if tmux_session_active() {
        spawn_sta_term(&format!(
            "exec tmux attach-session -t {STA_SESSION} 2>/dev/null || exec tmux new-session -s {STA_SESSION}"
        ));
        return Ok(());
    }
    if !(have_bin("foot") || have_bin("kitty") || have_bin("xterm")) {
        return Err("no terminal found (foot|kitty|xterm)".to_string());
    }
    let cwd = config::sta_cwd_default();
    let cwd_q = cwd.replace('\'', "'\\''");
    spawn_sta_term(&format!("cd '{cwd_q}' 2>/dev/null; exec sh"));
    Ok(())
}

#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    match provider.as_str() {
        "fish_audio" => stt::set_fish_api_key(&key),
        "anthropic" | "openai" | "google" | "openrouter" | "deepseek" | "qwen" | "moonshot" => {
            llm::set_cloud_key(&provider, &key)
        }
        _ => Err(format!("unknown provider '{provider}'")),
    }
}

#[tauri::command]
fn has_api_key(provider: String) -> bool {
    let entry = match provider.as_str() {
        "fish_audio" => keyring::Entry::new("whisp", "fish_audio"),
        "anthropic" | "openai" | "google" | "openrouter" | "deepseek" | "qwen" | "moonshot" => {
            keyring::Entry::new("whisp", provider.as_str())
        }
        _ => return false,
    };
    entry
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

/// Statut hooks par agent (Settings → Agents) : installé oui/non + chemin config.
#[tauri::command]
fn agent_hook_status() -> Vec<serde_json::Value> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let check = |id: &str, name: &str, path: std::path::PathBuf, marker: &str| {
        let installed = std::fs::read_to_string(&path)
            .map(|c| c.contains(marker))
            .unwrap_or(false);
        serde_json::json!({ "id": id, "name": name, "installed": installed, "path": path.display().to_string() })
    };
    vec![
        check("claude", "Claude Code", home.join(".claude/settings.json"), "whisp-hook"),
        check("codex", "Codex", home.join(".codex/config.toml"), "whisp"),
        check("gemini", "Gemini CLI", home.join(".gemini/settings.json"), "whisp-hook"),
        check("copilot", "Copilot CLI", home.join(".config/github-copilot/hooks.json"), "whisp"),
    ]
}

/// Lance `install-hook.sh --agent <id>` (Settings → Agents → Installer).
#[tauri::command]
fn install_agent_hooks(agent: String) -> Result<String, String> {
    let known = ["claude", "codex", "gemini", "copilot"];
    if !known.contains(&agent.as_str()) {
        return Err(format!("unknown agent '{agent}'"));
    }
    let script = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .map(|d| d.join("install-hook.sh"))
        .filter(|p| p.exists())
        .or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("install-hook.sh"))
        })
        .filter(|p| p.exists())
        .ok_or_else(|| "install-hook.sh introuvable".to_string())?;
    let out = std::process::Command::new("bash")
        .arg(&script)
        .arg("--agent")
        .arg(&agent)
        .output()
        .map_err(|e| format!("install-hook: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("install-hook: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Teste la connexion au serveur Ollama (Settings → Modèles locaux).
#[tauri::command]
fn test_ollama(url: Option<String>) -> Result<String, String> {
    let base = url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(config::ollama_url)
        .trim_end_matches('/')
        .to_string();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("ollama: {e}"))?;
    let tags: serde_json::Value = client
        .get(format!("{base}/api/tags"))
        .send()
        .map_err(|e| format!("ollama injoignable ({base}) : {e}"))?
        .error_for_status()
        .map_err(|e| format!("ollama status: {e}"))?
        .json()
        .map_err(|e| format!("ollama decode: {e}"))?;
    let n = tags.get("models").and_then(|m| m.as_array()).map(|a| a.len()).unwrap_or(0);
    Ok(format!("Ollama OK ({base}, {n} modèle(s))"))
}

#[tauri::command]
fn list_input_devices() -> Vec<String> {
    audio::list_input_devices()
}

#[tauri::command]
fn show_window(app: tauri::AppHandle, label: String) -> Result<(), String> {
    let w = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("unknown window '{label}'"))?;
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Answer one permission request from an explicit human click. `decision` is
/// `allow` | `always` | `deny`, or `question` with `answers` (question text →
/// picked label(s)) for AskUserQuestion cards. Never auto-allows: any other
/// value is a deny, and an unknown `request_id` is a no-op error.
#[tauri::command]
fn agent_decide(
    app: tauri::AppHandle,
    request_id: String,
    decision: String,
    answers: Option<std::collections::HashMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let id = request_id.trim();
    if id.is_empty() {
        return Err("empty request".to_string());
    }
    match decision.as_str() {
        "allow" | "always" | "deny" => {
            crate::agent_hook::answer(&app, id, &decision);
            Ok(())
        }
        "question" => {
            let answers = answers.filter(|a| !a.is_empty()).ok_or("empty answers")?;
            crate::agent_hook::answer_question(&app, id, &answers);
            Ok(())
        }
        _ => {
            // Unknown decision: safest is deny, never allow.
            crate::agent_hook::answer(&app, id, "deny");
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Tray + hotkeys + run
// ---------------------------------------------------------------------------

fn build_tray(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItem::with_id(app, "tray-open", "Ouvrir whisp", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let record = MenuItem::with_id(app, "tray-record", "Record", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let cycle = MenuItem::with_id(app, "tray-cycle", "Next mode", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let history_item = MenuItem::with_id(app, "tray-history", "History", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let settings = MenuItem::with_id(app, "tray-settings", "Settings", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&open, &sep, &record, &cycle, &history_item, &settings, &sep, &quit])
        .map_err(|e| e.to_string())?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("whisp — clic pour ouvrir")
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;
            // Left click = open the main window (Wayland appindicator
            // menus don't always open on click).
            if matches!(event, TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. }) {
                let app = tray.app_handle().clone();
                show_main_window(&app);
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app).map_err(|e| e.to_string())?;
    Ok(())
}

/// Show + focus the main window (settings). Used by tray click/menu.
fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        #[cfg(target_os = "linux")]
        let _ = w.set_always_on_top(false);
    }
}

fn on_tray_event(app: &tauri::AppHandle, id: &str) {
    match id {
        "tray-open" => show_main_window(app),
        "tray-record" => {
            let busy = app
                .try_state::<AppState>()
                .map(|s: State<'_, AppState>| {
                    matches!(
                        *s.session.lock().expect("session lock poisoned"),
                        SessionState::Recording(_) | SessionState::Starting
                    )
                })
                .unwrap_or(false);
            if busy {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let mode_id = app
                        .try_state::<AppState>()
                        .map(|s: State<'_, AppState>| current_mode_id(&s))
                        .unwrap_or_else(|| "message".to_string());
                    let mode = modes::get_mode(&mode_id).unwrap_or_else(|| {
                        modes::builtin_modes().into_iter().next().unwrap()
                    });
                    if let Err(e) = do_finish(app.clone(), mode).await {
                        emit_state(&app, RecStatus::Error, Some(e));
                    }
                });
            } else if let Err(e) = do_start(app) {
                emit_state(app, RecStatus::Error, Some(e));
            }
        }
        "tray-cycle" => {
            if let Some(state) = app.try_state::<AppState>() {
                let all = modes::list_modes();
                if !all.is_empty() {
                    let cur = state.mode_id.lock().expect("mode lock poisoned").clone();
                    let idx = all.iter().position(|m| m.id == cur).unwrap_or(0);
                    let next = all[(idx + 1) % all.len()].clone();
                    emit_mode_changed(app, &state, &next);
                }
            }
        }
        "tray-history" | "tray-settings" => {
            show_main_window(app);
        }
        "tray-quit" => app.exit(0),
        _ => {}
    }
}

fn register_hotkeys(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::ShortcutState;

    use std::sync::Arc;
    let handler_app = Arc::new(app.clone());
    let handler_app_cb = Arc::clone(&handler_app);
    if let Err(_) = app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app: &tauri::AppHandle, shortcut, event| {
                if event.state() == ShortcutState::Released
                    && *shortcut != hotkeys::push_to_talk_shortcut()
                {
                    return;
                }
                let pressed = event.state() == ShortcutState::Pressed;
                match hotkeys::match_action(shortcut, pressed) {
                    Some(hotkeys::HotkeyAction::ToggleRecording) => {
                        on_tray_event(&handler_app_cb, "tray-record");
                        let _ = app;
                    }
                    Some(hotkeys::HotkeyAction::PushToTalkPressed) => {
                        if let Err(e) = do_start(&handler_app_cb) {
                            emit_state(&handler_app_cb, RecStatus::Error, Some(e));
                        }
                    }
                    Some(hotkeys::HotkeyAction::PushToTalkReleased) => {
                        let app = (&*handler_app_cb).clone();
                        tauri::async_runtime::spawn(async move {
                            let mode_id = app
                                .try_state::<AppState>()
                                .map(|s: State<'_, AppState>| current_mode_id(&s))
                                .unwrap_or_else(|| "message".to_string());
                            let mode = modes::get_mode(&mode_id).unwrap_or_else(|| {
                                modes::builtin_modes().into_iter().next().unwrap()
                            });
                            if let Err(e) = do_finish(app.clone(), mode).await {
                                emit_state(&app, RecStatus::Error, Some(e));
                            }
                        });
                    }
                    Some(hotkeys::HotkeyAction::CycleMode) => {
                        on_tray_event(&handler_app_cb, "tray-cycle");
                    }
                    None => {}
                }
            })
            .build(),
    ) {
        return;
    }
    if let Err(e) = hotkeys::register_all(&handler_app) {
        emit_state(&handler_app, RecStatus::Error, Some(format!("hotkeys: {e}")));
    }
}

/// Local socket: control plane for the Hyprland bind AND the MCP server.
/// Protocol: one JSON object per line `{"cmd":"toggle"|"status"|"start"|"stop"|
/// "cycle_mode"|"set_mode"|"list_modes"|"list_mics"|"get_config"|"history", ...}`.
/// Reply: `{"ok":true,"data":...}` or `{"ok":false,"error":"..."}` + "\n".
/// Bare `toggle\n` (Hyprland bind) is accepted for backward compatibility.
/// One thread per connection (like `agent_hook`): a slow `stop` must not
/// stall `status`/`toggle`/`cancel` on other connections.
fn spawn_toggle_socket(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("whisp-toggle-sock".to_string())
        .spawn(move || {
            use std::os::unix::net::UnixListener;
            let path = std::env::temp_dir().join(format!(
                "whisp-{}.sock",
                std::env::var("UID").ok().and_then(|u| u.parse::<u32>().ok()).unwrap_or(1000)
            ));
            let _ = std::fs::remove_file(&path);
            let listener = match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(_) => {
                    return;
                }
            };
            for stream in listener.incoming().flatten() {
                // One thread per connection: a blocking `stop` must not stall
                // `status`/`toggle` on other connections.
                let app = app.clone();
                std::thread::Builder::new()
                    .name("whisp-toggle-conn".to_string())
                    .spawn(move || handle_socket_conn(&app, stream))
                    .ok();
            }
        })
        .ok();
}

/// One socket connection: read a single command line (MCP opens a fresh
/// connection per call), reply, close. Timeouts so a dead peer never parks
/// a thread forever.
fn handle_socket_conn(app: &tauri::AppHandle, stream: std::os::unix::net::UnixStream) {
    use std::io::{BufRead, BufReader, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(130)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    // Bound the line: no prompt-sized pastes on the control plane.
    const MAX_CMD_LEN: usize = 8192;
    if line.len() > MAX_CMD_LEN {
        let fail =
            serde_json::json!({"ok": false, "error": "command too long"}).to_string() + "\n";
        let mut s = reader.into_inner();
        let _ = s.write_all(fail.as_bytes());
        return;
    }
    let reply = handle_socket_cmd(app, line.trim());
    let mut s = reader.into_inner();
    let _ = s.write_all(reply.as_bytes());
    let _ = s.write_all(b"\n");
}

/// Execute one control-plane command, return the JSON reply body (no newline).
fn handle_socket_cmd(app: &tauri::AppHandle, line: &str) -> String {
    let ok = |data: serde_json::Value| {
        serde_json::json!({"ok": true, "data": data}).to_string()
    };
    let fail = |e: String| serde_json::json!({"ok": false, "error": e}).to_string();
    // Backward compat: bare "toggle" from the Hyprland bind.
    if line == "toggle" {
        on_tray_event(app, "tray-record");
        return ok(serde_json::json!({"toggled": true}));
    }
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return fail(format!("bad json: {e}")),
    };
    let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
    match cmd {
        "toggle" => {
            on_tray_event(app, "tray-record");
            ok(serde_json::json!({"toggled": true}))
        }
        "status" => {
            let st = app
                .try_state::<AppState>()
                .map(|s: tauri::State<'_, AppState>| {
                    let rec = s
                        .session
                        .lock()
                        .map(|g| matches!(*g, SessionState::Recording(_) | SessionState::Starting))
                        .unwrap_or(false);
                    let transcribing = s.transcribing.load(std::sync::atomic::Ordering::SeqCst);
                    let mode = s.mode_id.lock().map(|g| g.clone()).unwrap_or_default();
                    serde_json::json!({"recording": rec, "transcribing": transcribing, "mode": mode})
                });
            match st {
                Some(d) => ok(d),
                None => fail("no state".to_string()),
            }
        }
        "start" => match do_start(app) {
            Ok(()) => ok(serde_json::json!({"started": true})),
            Err(e) => fail(e),
        },
        "stop" => {
            // Non-blocking: if a transcription is already running, answer
            // {busy:true} instead of queuing behind it. Else take the handle
            // NOW (under the lock), reply {started:true} at once, and run
            // STT+LLM in the background; the result arrives via whisp://state
            // done and the MCP re-reads it with a `history` call.
            if is_transcribing(app) {
                return ok(serde_json::json!({"busy": true}));
            }
            let handle: Option<audio::RecordingHandle> = app
                .try_state::<AppState>()
                .and_then(|s: tauri::State<'_, AppState>| {
                    let mut guard = s.session.lock().ok()?;
                    match std::mem::replace(&mut *guard, SessionState::Idle) {
                        SessionState::Recording(h) => Some(h),
                        SessionState::Starting => {
                            *guard = SessionState::Starting;
                            None
                        }
                        SessionState::Idle => None,
                    }
                });
            let handle = match handle {
                Some(h) => h,
                None => return fail("not recording".to_string()),
            };
            let mode_id = app
                .try_state::<AppState>()
                .map(|s: tauri::State<'_, AppState>| current_mode_id(&s))
                .unwrap_or_else(|| "message".to_string());
            let mode = modes::get_mode(&mode_id).unwrap_or_else(|| {
                modes::builtin_modes().into_iter().next().unwrap()
            });
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                // Errors already surface via whisp://state Error inside; the
                // result itself arrives via whisp://state done + history.
                let _ = finish_from_handle(app2, mode, handle).await;
            });
            ok(serde_json::json!({"started": true}))
        }
        "cancel" => match do_cancel(app) {
            Ok(()) => ok(serde_json::json!({"cancelled": true})),
            Err(e) => fail(e),
        },
        "cycle_mode" => {
            on_tray_event(app, "tray-cycle");
            ok(serde_json::json!({"cycled": true}))
        }
        "set_mode" => {
            let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            match modes::get_mode(&id) {
                Some(m) => {
                    if let Some(st) = app.try_state::<AppState>() {
                        *st.mode_id.lock().expect("mode lock poisoned") = m.id.clone();
                    }
                    emit_state(app, RecStatus::Idle, Some(format!("mode: {}", m.id)));
                    ok(serde_json::json!({"mode": m.id}))
                }
                None => fail(format!("unknown mode '{id}'")),
            }
        }
        "list_modes" => ok(serde_json::json!(modes::list_modes())),
        "list_mics" => ok(serde_json::json!(audio::list_input_devices())),
        "get_config" => ok(serde_json::json!(config::load())),
        "set_config" => {
            let str_field = |k: &str| {
                v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string())
            };
            match config::set(
                str_field("popupPos"),
                str_field("sttModel"),
                str_field("theme"),
                str_field("language"),
                str_field("inputDevice"),
                str_field("staAgent"),
                str_field("staCommand"),
                str_field("staCwd"),
                v.get("staContext").and_then(|x| x.as_bool()),
                v.get("realtimePreview").and_then(|x| x.as_bool()),
                v.get("autoPaste").and_then(|x| x.as_bool()),
                v.get("activePills").and_then(|x| x.as_array()).map(|a| {
                    a.iter().filter_map(|y| y.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
                }),
                str_field("ollamaUrl"),
                str_field("ollamaModel"),
                str_field("chatProvider"),
                str_field("chatModel"),
                str_field("chatBaseUrl"),
                str_field("fishVoice"),
                v.get("staVoice").and_then(|x| x.as_bool()),
                str_field("popupSize"),
                v.get("cardOpacity").and_then(|x| x.as_f64()),
                v.get("popupMochi").and_then(|x| x.as_bool()),
            ) {
                Ok(()) => ok(serde_json::json!(config::load())),
                Err(e) => fail(e),
            }
        }
        "set_stt" => {
            let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let stt = v.get("stt").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let kind = match stt.as_str() {
                "local" => modes::SttKind::Local,
                "cloud_fish_audio" | "cloud" | "fish" => modes::SttKind::CloudFishAudio,
                _ => return fail(format!("unknown stt '{stt}' (local|cloud_fish_audio)")),
            };
            match modes::get_mode(&id) {
                Some(mut m) => {
                    m.stt = kind;
                    match modes::upsert_custom_mode(m) {
                        Ok(saved) => ok(serde_json::json!({"id": saved.id, "stt": stt})),
                        Err(e) => fail(e),
                    }
                }
                None => fail(format!("unknown mode '{id}'")),
            }
        }
        "history" => {
            let limit = v.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as u32;
            match history::list(limit.min(100)) {
                Ok(h) => ok(serde_json::json!(h)),
                Err(e) => fail(e),
            }
        }
        "" => fail("missing cmd".to_string()),
        other => fail(format!("unknown cmd '{other}'")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .manage(island::IslandState::new())
        .manage(agent_hook::PendingApprovals::default())
        .setup(|app| {
            register_hotkeys(app.handle());
            if build_tray(app.handle()).is_err() {
                // Best effort: tray is convenience, recorder works without it.
            }
            spawn_toggle_socket(app.handle().clone());
            agent_hook::spawn_agents_socket(app.handle().clone());
            agent_hook::ensure_relay_installed();
            island::spawn_idle_reaper(app.handle().clone());
            // Recorder starts hidden (see tauri.conf.json `visible: false`).
            Ok(())
        })
        .on_menu_event(|app, event| on_tray_event(app, event.id.as_ref()))
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed) {
                stt::stop_stt_daemon();
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            cancel_recording,
            recording_status,
            cycle_mode,
            get_current_mode,
            set_mode,
            list_modes,
            upsert_mode,
            delete_mode,
            list_history,
            search_history,
            get_history,
            delete_history,
            reprocess_history,
            get_config,
            set_config,
            sta_status,
            sta_open_terminal,
            island::island_list,
            island::island_dismiss,
            island::approval_list,
            island::island_term_view,
            island::island_term_send,
            agent_decide,
            has_api_key,
            list_input_devices,
            agent_hook_status,
            install_agent_hooks,
            test_ollama,
            voice::speak_text,
            shortcuts::list_shortcuts,
            shortcuts::reload_hyprland,
            appearance::set_popup_size,
            updates::app_version,
            updates::check_updates,
            llm::chat_command,
            llm::dictation_stats,
            show_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while running whisp")
        .run(|_app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                stt::stop_stt_daemon();
            }
        });
}
