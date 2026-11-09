//! Session + recorder state (extracted from `lib.rs`).
//! Single-lock lifecycle: Idle | Starting | Recording(handle).
//! `do_start`/`do_finish`/`do_cancel` all transition under this one lock,
//! so two concurrent toggles can never both spawn a capture.

use crate::audio;
use serde::Serialize;
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub session: std::sync::Mutex<SessionState>,
    pub mode_id: std::sync::Mutex<String>,
    pub agent_last: std::sync::Mutex<Option<String>>,
    /// True while `do_finish` transcription runs (STT+LLM, tens of seconds).
    pub transcribing: std::sync::atomic::AtomicBool,
    /// True while an Agent request runs (`omp -p` dans tmux, minutes).
    pub agent_running: std::sync::atomic::AtomicBool,
    /// Échap pendant la réponse vocale : le thread TTS s'arrête avant de jouer.
    pub voice_cancel: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: std::sync::Mutex::new(SessionState::Idle),
            mode_id: std::sync::Mutex::new("message".to_string()),
            agent_last: std::sync::Mutex::new(None),
            transcribing: std::sync::atomic::AtomicBool::new(false),
            agent_running: std::sync::atomic::AtomicBool::new(false),
            voice_cancel: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Lock that recovers from poisoning (a panicked holder must not brick
    /// the recorder forever in release).
    pub fn session_lock(&self) -> std::sync::MutexGuard<'_, SessionState> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn mode_lock(&self) -> std::sync::MutexGuard<'_, String> {
        self.mode_id.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub enum SessionState {
    Idle,
    /// `do_start` holds this from the `spawn_session` call until the handle
    /// is stored; any peer seeing it refuses with "already recording".
    Starting,
    Recording(audio::RecordingHandle),
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
    pub fn as_str(self) -> &'static str {
        match self {
            RecStatus::Idle => "idle",
            RecStatus::Recording => "recording",
            RecStatus::Processing => "processing",
            RecStatus::Done => "done",
            RecStatus::Error => "error",
        }
    }
}

pub fn current_mode_id(state: &State<AppState>) -> String {
    state
        .mode_id
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn emit_state(app: &tauri::AppHandle, status: RecStatus, message: Option<String>) {
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
pub fn emit_mode_changed(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    next: &crate::modes::Mode,
) {
    *state.mode_id.lock().unwrap_or_else(|e| e.into_inner()) = next.id.clone();
    let transcribing = state.transcribing.load(std::sync::atomic::Ordering::SeqCst);
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

pub fn show_recorder(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("recorder") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_recorder(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("recorder") {
        let _ = w.hide();
    }
}
