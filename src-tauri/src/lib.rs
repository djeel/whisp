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
mod caelestia;
mod config;
mod history;
mod hotkeys;
mod llm;
mod modes;
mod shortcuts;
mod stt;
mod updates;
mod voice;

use std::sync::Mutex;
use std::time::Duration;

use crate::llm::Llm as _;
use crate::stt::Stt as _;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

pub struct AppState {
    session: Mutex<SessionState>,
    mode_id: Mutex<String>,
    agent_last: Mutex<Option<String>>,
    /// True while `do_finish` transcription runs (STT+LLM, tens of seconds).
    /// Lets a concurrent `stop` answer `{busy:true}` instead of blocking.
    transcribing: std::sync::atomic::AtomicBool,
    /// True while an Agent request runs (`omp -p` dans tmux, minutes).
    /// Une seule requête à la fois : sinon sorties entrelacées + CPU 100 %.
    agent_running: std::sync::atomic::AtomicBool,
    /// Échap pendant la réponse vocale : le thread TTS s'arrête avant de jouer.
    voice_cancel: std::sync::atomic::AtomicBool,
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
            agent_last: Mutex::new(None),
            transcribing: std::sync::atomic::AtomicBool::new(false),
            agent_running: std::sync::atomic::AtomicBool::new(false),
            voice_cancel: std::sync::atomic::AtomicBool::new(false),
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

fn show_recorder(app: &tauri::AppHandle) {
    // show SANS focus : la popup ne vole jamais le curseur. Le paste Ctrl+V
    // part donc dans l'app cible, pas besoin de hide/show autour du collage
    // (c'est ce hide/show qui fermait + recentrait la popup à chaque validation).
    if let Some(w) = app.get_webview_window("recorder") {
        let _ = w.show();
        // Hyprland ignore les configure (size/pos) d'une fenêtre jamais
        // focusée : le focus autorise les resizes suivants (fit_window).
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
                        // 12 s suffisent au preview : copie bornée, lock tenu
                        // des millisecondes (le snapshot plein figeait `stop`).
                        SessionState::Recording(h) => Some(h.snapshot_last(16_000 * 12)),
                        SessionState::Idle | SessionState::Starting => None,
                    })
                });
                let pcm = match pcm {
                    Some(p) if p.len() >= 32_000 => p,
                    _ => break, // session gone (stop/cancel): exit
                };
                // Pas de re-copie : `snapshot_last` a déjà borné à 12 s.
                if let Some(text) = stt::transcribe_partial(&pcm) {
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
        let mut guard = state.session.lock().unwrap_or_else(|e| e.into_inner());
        // Toujours Idle après cancel (même si déjà Idle : Échap ferme la popup
        // dans tous les cas, y compris sur l'écran done/processing).
        match *guard {
            SessionState::Idle => {}
            SessionState::Starting | SessionState::Recording(_) => *guard = SessionState::Idle,
        }
    }
    // Échap = tout s'arrête : Agent en cours (Ctrl-C tmux + flag), réponse
    // vocale en cours (le thread TTS vérifie le flag), puis hide.
    // Deadline : Échap ne doit jamais figer l'app si tmux est coincé.
    let _ = run_timed(&["tmux", "send-keys", "-t", AGENT_SESSION, "C-c"], Duration::from_secs(3));
    state
        .agent_running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state
        .voice_cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    // Coupe le son TTS en cours (réponse voice qui traîne).
    crate::voice::kill_playback();
    hide_recorder(app);
    emit_state(app, RecStatus::Idle, None);
    Ok(())
}
/// True while `do_finish` runs the STT+LLM pipeline (not just draining).
fn is_transcribing(app: &tauri::AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|s: State<'_, AppState>| s.transcribing.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(false)
}

/// Drain the session, transcribe -> refine -> history -> clipboard/paste.
/// Take is done under the lock, `finish()` (100 ms sleep) outside it.
async fn do_finish(app: tauri::AppHandle, entry_mode: modes::Mode) -> Result<StopResult, String> {
    // Take the handle under the single lock; refus propre if Starting/Idle.
    let handle: audio::RecordingHandle = {
        let state = app.state::<AppState>();
        let mut guard = state.session.lock().unwrap_or_else(|e| e.into_inner());
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
    let is_agent = entry_mode.id == "agent";
    let is_voice = entry_mode.id == "voice";
    let mode_id_done = entry_mode.id.clone();
    let pipeline: Result<(String, String), String> =
        tauri::async_runtime::spawn_blocking(move || {
            let stt = stt::select_stt(entry_mode.stt);
            // Global dictation language wins over mode "auto" (avoids
            let lang = if entry_mode.language == "auto" {
                config::dictation_language()
            } else {
                entry_mode.language.clone()
            };
            let tx = stt.transcribe(&pcm, &lang)?;
            let llm = llm::select_llm(entry_mode.llm);
            // Dégradation : si le raffinage échoue (Ollama injoignable),
            // on colle le brut plutôt que de tout perdre (WAV + texte).
            let refined = match llm.refine(&tx.text, &entry_mode) {
                Ok(r) => r.text,
                Err(e) => {
                    eprintln!("[whisp] refine failed, raw fallback: {e}");
                    tx.text.clone()
                }
            };
            let wav = audio::encode_wav_16k_mono(&pcm);
            let entry = history::insert(&entry_mode.id, &tx.text, &refined, Some(&wav))?;
            let _ = entry;
            Ok::<(String, String), String>((tx.text, refined))
        })
        .await
        .map_err(|e| format!("pipeline task: {e}"))?;
    // Pipeline done (ok or err): clear the flag before propagating.
    if let Some(state) = app.try_state::<AppState>() {
        state
            .transcribing
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    // Point d'émission d'erreur UNIQUE : un pipeline en échec ne reste
    // jamais en `processing` (freeze constaté : le commentaire disait que
    // les erreurs passent par state Error, mais rien ne l'émettait ici).
    let (raw, text_final): (String, String) = match pipeline {
        Ok(t) => t,
        Err(e) => {
            emit_state(&app, RecStatus::Error, Some(e.clone()));
            return Err(e);
        }
    };
    // Ordre critique : hide JUSTE avant le paste (rend le focus à l'app
    // cible), paste, puis re-show SANS focus. Avant : hide 200 lignes plus
    // Ctrl+V partait dans le vide. La popup reste ouverte après validation.
    let agent_reused = if is_agent {
        // Garde : une seule requête Agent à la fois. `swap(true)` atomique :
        // si un `omp -p` tourne déjà dans tmux, on refuse au lieu
        // d'entrelacer deux sorties (CPU 100 %, impression de boucle).
        let was_running = app
            .try_state::<AppState>()
            .map(|s: State<'_, AppState>| {
                s.agent_running
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
            })
            .unwrap_or(false);
        if was_running {
            emit_state(
                &app,
                RecStatus::Error,
                Some("Agent déjà en cours, attends la fin".to_string()),
            );
            return Err("Agent déjà en cours".to_string());
        }
        let out = match launch_agent(&text_final) {
            Ok((reused, full)) => {
                if let Some(prompt) = full {
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut g) = state.agent_last.lock() {
                            *g = Some(prompt);
                        }
                    }
                    // Agent modulaire inline (popup) : event vers le recorder,
                    // plus d'island. Le module affiche working + statut.
                    let _ = app.emit(
                        "whisp://agent",
                        serde_json::json!({ "state": "working", "agent": config::agent_id() }),
                    );
                }
                // Réponse vocale Agent (Fish TTS) quand `agent_voice` est activé.
                voice::maybe_speak_agent(&text_final);
                reused
            }
            Err(e) => {
                // Spawn failed twice: keep the request in history, toast the
                // reason through the recorder state message, no silent drop.
                // `e` already carries the cause in parentheses (actionable);
                // propagate it instead of a bare generic string.
                emit_state(&app, RecStatus::Error, Some(e.clone()));
                if let Some(state) = app.try_state::<AppState>() {
                    state
                        .agent_running
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                }
                return Err(format!("Agent introuvable, vérifie Configuration ({e})"));
            }
        };
        if let Some(state) = app.try_state::<AppState>() {
            state
                .agent_running
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        out
    } else if is_voice {
        // Mode voice = conversation pure : PAS de paste, PAS de clipboard.
        // La question reste à l'écran (event), la réponse arrive en vocal.
        // Avant : collait la question comme un mode message.
        let _ = app.emit(
            "whisp://voice-question",
            serde_json::json!({ "text": text_final }),
        );
        false
    } else if config::auto_paste() {
        // PAS de hide/show : la popup ne se ferme ni ne saute. Le paste via
        // wtype envoie Ctrl+V à la fenêtre focus — or la popup a le focus...
        // donc on minimise : pas de hide (qui ferme), juste le paste. Si la
        // popup a le focus, wtype colle dedans : on refocus d'abord la cible
        // mémorisée ? Non — Hyprland garde le focus précédent quand la popup
        // est `focus: false`. Le vrai fix est côté fenêtre : pas de focus.
        paste_text(&text_final);
        false
    } else {
        // Collage auto désactivé : texte en presse-papiers + historique uniquement.
        copy_to_clipboard(&text_final);
        false
    };
    // Mode voice : conversation vocale. Le LLM répond, Fish TTS lit la réponse.
    // Avant : le mode nettoyait juste le texte, aucune réponse vocale.
    if is_voice && !text_final.trim().is_empty() {
        let app_evt = app.clone();
        voice::maybe_speak_voice_reply(&text_final, move |playing: bool| {
            use tauri::Emitter;
            let _ = app_evt.emit(
                "whisp://tts",
                serde_json::json!({ "state": if playing { "playing" } else { "done" } }),
            );
        });
    }
    emit_state(&app, RecStatus::Done, None);
    // PAS de retour auto vers idle : le passage done → idle re-rendait la
    // popup (impression de ferme/réouvre). Elle reste sur done jusqu'à la
    // prochaine dictée (recording) ou Échap (idle via cancel).

    Ok(StopResult {
        transcript_raw: raw,
        text_final,
        // #7: entry mode captured at stop, not the (maybe cycled) current one.
        mode: mode_id_done,
        reused: agent_reused,
    })
}

// ---------------------------------------------------------------------------
// Agent (speech-to-agent): one persistent terminal, reused dictation after
// dictation (hybrid smart mode).
//
// - Session check: tmux session `whisp-agent` when tmux exists, else a running
//   terminal titled `whisp-agent` (pgrep).
// - Session live -> the new request is injected (`tmux send-keys`); the
//   existing terminal is focused, never duplicated.
// - No session -> created (foot, fallback kitty/xterm) running the agent.
// - Context joined to the prompt: `Contexte: dossier <cwd>. Demande: <text>`
//   (+ screenshot path when `grim` captured one). User speech is never logged.
// ---------------------------------------------------------------------------

/// Watchdog du thread principal GTK : si TOUTES les fenêtres figent,
/// c'est ici que ça se voit (commande synchrone bloquée, tmux coincé…).
/// Ping `run_on_main_thread` chaque seconde : >200 ms de latence = warning,
/// >2 s sans réponse = MAIN THREAD BLOCKED (puis pstree/gdb côté shell).
/// Coût : un thread dormant + un round-trip IPC/s.
fn spawn_main_thread_watchdog(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("whisp-watchdog".to_string())
        .spawn(move || loop {
            let (tx, rx) = std::sync::mpsc::channel();
            let t = std::time::Instant::now();
            let _ = app.run_on_main_thread(move || {
                let _ = tx.send(());
            });
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(_) if t.elapsed() > std::time::Duration::from_millis(200) => {
                    eprintln!("[whisp] main thread lag {:?}", t.elapsed());
                }
                Err(_) => {
                    eprintln!(
                        "[whisp] MAIN THREAD BLOCKED >2s (pstree -p $(pidof whisp) pour le coupable)"
                    );
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        })
        .ok();
}

/// Persistent agent session (tmux name + terminal title).
const AGENT_SESSION: &str = "whisp-agent";

 fn have_bin(name: &str) -> bool {
    // Cache process-wide : `command -v` spawnait un `sh` à chaque poll
    // (agent_status + agent_last_output à 1 Hz = 2 sh/s). Le PATH ne
    // change pas pendant la vie du process : scan direct, zéro spawn.
    static CACHE: std::sync::LazyLock<std::collections::HashMap<String, bool>> =
        std::sync::LazyLock::new(|| {
         let mut m = std::collections::HashMap::new();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
        dirs.push("/usr/bin".into());
        dirs.push("/usr/local/bin".into());
        for bin in [
            "tmux", "hyprctl", "grim", "wl-copy", "wtype", "ydotool", "xdotool", "foot", "kitty",
            "xterm", "pgrep", "pkill", "bash",
        ] {
            m.insert(
                bin.to_string(),
                dirs.iter().any(|d| {
                    let p = d.join(bin);
                    std::fs::metadata(&p)
                        .map(|md| {
                            use std::os::unix::fs::PermissionsExt;
                            md.is_file() && md.permissions().mode() & 0o111 != 0
                        })
                        .unwrap_or(false)
                }),
            );
        }
        m
    });
    if let Some(&hit) = CACHE.get(name) {
        return hit;
    }
    // Binaire hors liste : fallback spawn (rare, jamais sur le hot path).
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
    // Deadline 5 s : un serveur tmux coincé figeait le thread principal
    // (freeze constaté sur agent_status pollé à 1 Hz).
    have_bin("tmux")
        && run_timed(&["tmux", "has-session", "-t", AGENT_SESSION], Duration::from_secs(5)).is_ok()
 }

/// Deadline for a local tmux control command (`has-session`,
/// `display-message`, `new-session`, `send-keys`, `kill-session`). These are
/// local IPC to the tmux server: milliseconds when healthy. The old 120 s
/// deadline also covered `send-keys`, parking the dictation worker 2 min on
/// a wedged server — 10 s is already generous. The agent run itself
/// (`omp -p …`) is never waited on (fire-and-forget into the session
/// shell), so no long timeout is needed anywhere here.
const TMUX_TIMEOUT: Duration = Duration::from_secs(10);

/// True when the tmux *server* answers for the session, not just that the
/// session name exists. `has-session` can succeed while the server is
/// wedged; `display-message` forces a round-trip under a short deadline.
fn tmux_session_responsive() -> bool {
    if !have_bin("tmux") {
        return false;
    }
    run_timed(
        &["tmux", "display-message", "-p", "-t", AGENT_SESSION, "#S"],
        TMUX_TIMEOUT,
    )
    .is_ok()
}

/// Kill a dead `whisp-agent` session and create a fresh shell. Retries once
/// (the daemon can be mid-restart when the first spawn races it). `Err`
/// names the cause so the caller can surface an actionable message.
fn recreate_agent_session() -> Result<(), String> {
    let _ = run_timed(&["tmux", "kill-session", "-t", AGENT_SESSION], TMUX_TIMEOUT);
    let mut created = run_timed(
        &["tmux", "new-session", "-d", "-s", AGENT_SESSION],
        TMUX_TIMEOUT,
    );
    if created.is_err() {
        created = run_timed(
            &["tmux", "new-session", "-d", "-s", AGENT_SESSION],
            TMUX_TIMEOUT,
        );
    }
    created.map_err(|e| {
        format!(
            "session tmux `{AGENT_SESSION}` incréable ({e}) — vérifie que tmux est installé et répond (`tmux ls`)"
        )
    })
}

/// A visible agent terminal is already open (foot/kitty/xterm titled whisp-agent).
 fn agent_term_visible() -> bool {
    // Deadline : pgrep coincé (sysfs lent) ne doit pas figer le poll 1 Hz.
    run_timed(&["pgrep", "-f", AGENT_SESSION], Duration::from_secs(5)).is_ok()
 }
 
 /// True when a previous Agent dictation left a live session behind.
 fn agent_session_active() -> bool {
     tmux_session_active() || agent_term_visible()
 }
 
 /// Bring the existing agent terminal forward (Hyprland) instead of opening
 /// a second one. Best effort.
 fn focus_agent_term() {
    // Best effort : un hyprctl coincé ne doit jamais figer l'UI.
    let _ = run_timed(
        &["hyprctl", "dispatch", "focuswindow", &format!("title:{AGENT_SESSION}")],
        Duration::from_secs(3),
    );
 }

/// Open the visible agent terminal running `shell_cmd` (a `sh -c` body).
/// foot first, then kitty, then xterm. Silent when no terminal exists.
/// foot: `--window-size-chars` + `pad=2x2` override (le pad 25x25 global +
/// `resize-by-cells` coupait le bas du prompt sous le padding).
fn spawn_agent_term(shell_cmd: &str) {
    let null = || std::process::Stdio::null();
    if have_bin("foot") {
        let _ = std::process::Command::new("foot")
            .args([
                "--title",
                AGENT_SESSION,
                "-o",
                "pad=2x2",
                "-W",
                "100x28",
                "sh",
                "-c",
                shell_cmd,
            ])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    } else if have_bin("kitty") {
        let _ = std::process::Command::new("kitty")
            .args(["--title", AGENT_SESSION, "sh", "-c", shell_cmd])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    } else if have_bin("xterm") {
        let _ = std::process::Command::new("xterm")
            .args(["-title", AGENT_SESSION, "-e", "sh", "-c", shell_cmd])
            .stdin(null())
            .stdout(null())
            .stderr(null())
            .spawn();
    }
}

/// Active project folder: first existing directory among path-like tokens in
/// the focused window's class/title (`hyprctl activewindow -j`); the
/// configured default otherwise. Best effort, never blocks dictation.
fn resolve_agent_cwd() -> String {
    let default = config::agent_cwd_default();
    if !config::agent_context_enabled() {
        return default;
    }
    // Deadline 3 s : hyprctl coincé (service sans WAYLAND_DISPLAY) = défaut.
    let text = run_timed_out(&["hyprctl", "activewindow", "-j"], Duration::from_secs(3)).ok();
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
fn capture_agent_screenshot() -> Option<String> {
    if !config::agent_context_enabled() || !have_bin("grim") {
        return None;
    }
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|r| format!("{r}/whisp-agent-screen.png"))
        .unwrap_or_else(|_| "/tmp/whisp-agent-screen.png".to_string());
    // Deadline 10 s : capture plein écran sur disque lent.
    let ok = run_timed(&["grim", &path], Duration::from_secs(10)).is_ok();
    if ok {
        Some(path)
    } else {
        None
    }
}

/// Longest dictated request carried in one tmux `send-keys` line. A full
/// untruncated dictation can be tens of KB (long ramble), which the tmux
/// input buffer swallows silently — the agent then runs with a cut prompt
/// and no error. ~4000 chars keep the whole command line well under the
/// buffer while preserving any realistic request.
const AGENT_PROMPT_MAX_CHARS: usize = 4000;

/// Test seam: pure prompt builder (no config, no FS except the screenshot
/// existence check) so unit tests can pin truncation/screenshot behaviour.
fn agent_prompt_inner(request: &str, cwd: &str, shot: Option<&str>, context: bool) -> String {
    if !context {
        return request.to_string();
    }
    // Char-boundary truncation (never split a multi-byte char).
    let req: String = request.chars().take(AGENT_PROMPT_MAX_CHARS).collect();
    // Folder basename in addition to the full path ("whisp
    // (/home/u/Projets/whisp)"): the agent resolves relative paths against
    // the full path, the basename disambiguates at a glance.
    let folder = std::path::Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .map(|n| format!("{n} ({cwd})"))
        .unwrap_or_else(|| cwd.to_string());
    let head = format!("Contexte: dossier {folder}.");
    // The path is only useful if the file is really there (grim may have
    // failed between capture and prompt): a dangling path makes the agent
    // hallucinate an image instead of ignoring it.
    match shot {
        Some(p) if std::path::Path::new(p).is_file() => {
            format!("{head} Capture d'écran: {p}. Demande: {req}")
        }
        _ => format!("{head} Demande: {req}"),
    }
}

/// Agent prompt: `Contexte: dossier <basename> (<cwd>). [Capture d'écran: <path>.]
/// Demande: <transcript>` (request tronquée à ~4000 car). No agent preset
/// takes an image flag today, so the screenshot travels as a path inside
/// the text prompt — only when the file really exists. Plain transcript
/// when context is off.
fn agent_prompt(request: &str, cwd: &str, shot: Option<&str>) -> String {
    agent_prompt_inner(request, cwd, shot, config::agent_context_enabled())
}

/// Agent invocation for one prompt (single line: `tmux send-keys` types it
/// literally). Presets: `omp -p`, `claude`, `codex exec`, or the custom
/// `agent_command` template with `{prompt}` (shell-quoted).
fn agent_inner_command(agent: &str, template: &str, prompt: &str) -> String {
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let quoted = format!("'{}'", flat.replace('\'', "'\\''"));
    if !template.is_empty() {
        return template.replace("{prompt}", &quoted);
    }
    // Binaire manquant -> fallback omp (évite terminal qui s'ouvre et meurt).
    let effective = preset_agent_bin(agent, have_bin);
    match effective {
        "claude" => format!("claude {quoted}"),
        "codex" => format!("codex exec {quoted}"),
        _ => format!("omp -p {quoted}"),
    }
}

/// Preset agent id -> binary actually invoked. Unknown ids and missing
/// binaries fall back to `omp` (never spawn a terminal that dies at once).
/// `have` is a seam so unit tests pin the fallback without touching PATH.
fn preset_agent_bin(agent: &str, have: impl Fn(&str) -> bool) -> &str {
    let bin = match agent {
        "claude" => "claude",
        "codex" => "codex",
        "omp" => "omp",
        _ => return "omp",
    };
    if have(bin) {
        agent
    } else {
        "omp"
    }
}

/// Run one shell command with a deadline. `Err` on spawn failure, non-zero
/// exit, or timeout — the caller retries once, then surfaces the message.
/// Callers pass a short deadline (see `TMUX_TIMEOUT`): tmux control
/// commands are local IPC and answer in milliseconds, so a timeout always
/// means a wedged server, never a slow agent run (the run itself is
/// fire-and-forget, never waited on).
/// Exécute un process avec deadline. `pub(crate)` : les modules commandes
/// (shortcuts, voice) partagent le même garde-fou anti-freeze.
pub(crate) fn run_timed(argv: &[&str], timeout: Duration) -> Result<(), String> {
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
        match child
            .try_wait()
            .map_err(|e| format!("wait {}: {e}", argv[0]))?
        {
            Some(status) if status.success() => return Ok(()),
            Some(status) => return Err(format!("{} exited with {status}", argv[0])),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} timed out after {}s",
                        argv[0],
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Même contrat que `run_timed` mais capture stdout (tronqué à 64 Ko).
/// Utilisé par les commandes Tauri qui lisent un process (hyprctl, tmux
/// capture-pane) : sans deadline, un serveur coincé fige le thread principal
/// GTK et TOUTES les fenêtres (freeze constaté sur les polls à 1 Hz).
pub(crate) fn run_timed_out(argv: &[&str], timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    if argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut child = std::process::Command::new(argv[0])
        .args(&argv[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", argv[0]))?;
    let mut stdout_taken = child.stdout.take();
    // Lecture dans un thread dédié : le pipe ne bloque jamais le wait.
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut acc = Vec::new();
        if let Some(so) = stdout_taken.as_mut() {
            loop {
                match so.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        acc.extend_from_slice(&buf[..n]);
                        if acc.len() > 65536 {
                            break;
                        }
                    }
                }
            }
        }
        String::from_utf8_lossy(&acc).into_owned()
    });
    let start = std::time::Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|e| format!("wait {}: {e}", argv[0]))?
        {
            Some(status) if status.success() => {
                let mut out = reader.join().unwrap_or_default();
                if out.len() > 65536 {
                    out.truncate(65536);
                }
                return Ok(out);
            }
            Some(status) => {
                let _ = reader.join();
                let _ = child.wait();
                return Err(format!("{} exited with {status}", argv[0]));
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(format!("{} timed out after {}s", argv[0], timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
/// Run one Agent request through the persistent agent session.
/// Returns `(reused, full_prompt)`: `reused` is true when the request was
/// injected into the live session; `full_prompt` is the context-carrying
/// prompt (None when the transcript was empty). `Err` when the spawn
/// failed twice (retry once) or no agent binary/terminal is available —
/// history keeps the request either way; user speech is never logged.
fn launch_agent(request: &str) -> Result<(bool, Option<String>), String> {
    let request = request.trim();
    if request.is_empty() {
        return Ok((false, None));
    }
    let cwd = resolve_agent_cwd();
    let shot = capture_agent_screenshot();
    let prompt = agent_prompt(request, &cwd, shot.as_deref());
    let agent = config::agent_id();
    let template = config::agent_command_template();
    let inner = agent_inner_command(&agent, &template, &prompt);
    // Run from the project folder even if the session shell moved elsewhere.
    let inner = format!("cd '{}' && {inner}", cwd.replace('\'', "'\\''"));

    if have_bin("tmux") {
        let mut live = tmux_session_active();
        if live && !tmux_session_responsive() {
            // `has-session` dit vrai mais le serveur ne répond plus (shell gelé,
            // serveur coincé) : injecter dans le vide perdrait la demande sans
            // erreur. Tuer + recréer plutôt que d'envoyer dans le vide.
            live = recreate_agent_session()
                .map(|_| tmux_session_responsive())
                .unwrap_or(false);
            if !live {
                return Err("Agent introuvable, vérifie Configuration (session tmux gelée, recréation impossible — relance tmux ou redémarre whisp)".to_string());
            }
        }
        if !live {
            // Fresh persistent session: a plain shell (stays alive between
            // requests, so the next dictation can inject into it).
            recreate_agent_session()
                .map_err(|e| format!("Agent introuvable, vérifie Configuration ({e})"))?;
            live = false;
        }
        if tmux_session_responsive() {
            // Inject the request into the session shell (retry once on failure).
            let mut sent = run_timed(
                &["tmux", "send-keys", "-t", AGENT_SESSION, &inner, "Enter"],
                TMUX_TIMEOUT,
            );
            if sent.is_err() {
                sent = run_timed(
                    &["tmux", "send-keys", "-t", AGENT_SESSION, &inner, "Enter"],
                    TMUX_TIMEOUT,
                );
            }
            if let Err(e) = sent {
                return Err(format!("Agent introuvable, vérifie Configuration (envoi vers tmux `{AGENT_SESSION}` refusé : {e} — rouvre le terminal agent)"));
            }
            if agent_term_visible() {
                focus_agent_term();
            } else {
                spawn_agent_term(&format!(
          "exec tmux attach-session -t {AGENT_SESSION} 2>/dev/null || exec tmux new-session -s {AGENT_SESSION}"
        ));
                // Laisse le terminal apparaître, puis ramène-le devant.
                std::thread::sleep(std::time::Duration::from_millis(600));
                focus_agent_term();
            }
            return Ok((live, Some(prompt)));
        }
        // tmux present but unusable: fall through to a plain terminal.
    } else {
        // Pas de tmux : pas d'injection fiable — terminal simple avec la
        // demande dedans (mode dégradé, une fenêtre par demande).
        if agent_term_visible() {
            focus_agent_term();
        }
        if !(have_bin("foot") || have_bin("kitty") || have_bin("xterm")) {
            return Err("Agent introuvable, vérifie Configuration (ni tmux ni terminal foot|kitty|xterm — installe tmux ou foot)".to_string());
        }
        spawn_agent_term(&format!(
            "{inner}; echo; echo '[whisp-agent done — Enter to close]'; read _"
        ));
        return Ok((false, Some(prompt)));
    }

    // No tmux: no reliable injection, run the request in its own terminal.
    if agent_term_visible() {
        focus_agent_term();
    }
    if !(have_bin("foot") || have_bin("kitty") || have_bin("xterm")) {
        return Err("Agent introuvable, vérifie Configuration (aucun terminal foot|kitty|xterm — installe tmux ou foot)".to_string());
    }
    spawn_agent_term(&format!(
        "{inner}; echo; echo '[whisp-agent done — Enter to close]'; read _"
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
    // Le hide a déjà rendu le focus à l'app cible ; laisser le compositor
    // finir avant d'envoyer Ctrl+V (sinon la frappe part dans le vide).
    std::thread::sleep(std::time::Duration::from_millis(350));
    // 2. Paste via Ctrl+V (unicode-safe : les accents passent par le
    // presse-papiers, contrairement à `ydotool type` qui les massacre).
    // `ydotool key ctrl+v` ne marche PAS (keycodes bruts exigés : 29 = Ctrl,
    // 47 = V) ; Wayland natif (wtype, virtual-keyboard) en premier.
    let run_quiet = |bin: &str, args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let pasted = run_quiet("wtype", &["-M", "ctrl", "-k", "v", "-m", "ctrl"])
        || run_quiet("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"])
        || run_quiet("xdotool", &["key", "ctrl+v"]);
    if !pasted {
        // Plus de collage silencieux : même presse-papiers OK, le toggle est ON.
        eprintln!("[whisp] paste failed (wtype/ydotool/xdotool): clipboard_ok={wl_ok}");
    }
    // Text stays in history either way (best effort).
}

// ---------------------------------------------------------------------------
// Tauri commands (frontend contract)
// ---------------------------------------------------------------------------

/// Resize demandé par le frontend (RO) : setSize + position, CÔTÉ BACKEND.
/// Le JS window API ne voit pas les fenêtres ici ; le backend, si.
/// Garde anti-boucle : ignore si Δ < 2 px.
static LAST_FIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[tauri::command]
fn fit_window(app: tauri::AppHandle, w: f64, h: f64) -> Result<String, String> {
    use tauri::Manager;
    let Some(win) = app.get_webview_window("recorder") else {
        return Err("no recorder".to_string());
    };
    let cur = win.inner_size().map_err(|e| e.to_string())?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let cw = cur.width as f64 / scale;
    let ch = cur.height as f64 / scale;
    if (cw - w).abs() < 2.0 && (ch - h).abs() < 2.0 {
        return Ok("same".to_string());
    }
    // tao annonce set_window_geometry avec la taille du BUFFER (périmée),
    // pas la demandée : forcer via set_min_size (déclaratif, le compositor
    // suit par configure), puis set_size, puis libérer le min.
    let _ = win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(w, h))));
    let _ = win.set_size(tauri::LogicalSize::new(w, h));
    // Ne PAS libérer le min aussitôt : tao coalesce les requêtes et le
    // None écrase le Some avant l'envoi (prouvé : que des (0,0) sur le wire).
    // Le prochain fit écrasera ce min par le sien ; à la fermeture, cancel
    // remet 200 via le même chemin.
    // Mémorise APRÈS succès vérifié : un resize ignoré par le compositor
    // ne doit jamais bloquer les réessais ("same" fantôme).
    {
        let sc = win.scale_factor().unwrap_or(1.0);
        if let Ok(sz) = win.inner_size() {
            let aw = sz.width as f64 / sc;
            let ah = sz.height as f64 / sc;
            if (aw - w).abs() < 4.0 && (ah - h).abs() < 4.0 {
                let key = ((w as u64) << 32) | (h as u64 & 0xFFFF_FFFF);
                LAST_FIT.store(key, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
    // Position : NE PAS retoucher si la fenêtre est déjà bien placée.
    // L'ancien code reposait set_position à chaque fit (même taille) : le
    // clamp sur nouvelle hauteur remontait la popup à chaque changement de mode.
    // Ici : on ne repositionne que si ça dépasse, en gardant le HAUT fixe
    // (y actuel conservé, clampé écran) — la fenêtre grandit vers le bas.
    if let Ok(pos) = win.outer_position() {
        let sc = win.scale_factor().unwrap_or(1.0);
        let px = pos.x as f64 / sc;
        let py = pos.y as f64 / sc;
        if let Ok(Some(monitor)) = win.current_monitor() {
            use tauri::Monitor;
            let factor = monitor.scale_factor();
            let mw = monitor.size().width as f64 / factor;
            let mx = monitor.position().x as f64 / factor;
            let my = monitor.position().y as f64 / factor;
            let mh = monitor.size().height as f64 / factor;
            let cx = px.clamp(mx, (mx + mw - w).max(mx));
            let cy = py.clamp(my, (my + mh - h).max(my));
            // Bouge seulement si le clamp corrige vraiment (> 1 px) : sinon
            // chaque fit ferait "sauter" la popup (arrondis, race compositor).
            if (cx - px).abs() > 1.0 || (cy - py).abs() > 1.0 {
                let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                    x: cx,
                    y: cy,
                }));
                return Ok(format!("resized {w}x{h} @clamped"));
            }
        }
        return Ok(format!("resized {w}x{h}"));
    }
    Ok(format!("resized {w}x{h}"))
}

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
    let mode = modes::get_mode(&mode_id)
        .unwrap_or_else(|| modes::builtin_modes().into_iter().next().unwrap());
    do_finish(app, mode).await
}

#[tauri::command]
fn cancel_recording(app: tauri::AppHandle) -> Result<(), String> {
    do_cancel(&app)
}

#[tauri::command]
fn recording_status(app: tauri::AppHandle, state: State<'_, AppState>) -> String {
    let _ = &app;
    if state.transcribing.load(std::sync::atomic::Ordering::SeqCst) {
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
fn set_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<modes::Mode, String> {
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
fn delete_history(id: String) -> Result<(), String> {
    history::delete(&id)
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
    agent: Option<String>,
    agent_command: Option<String>,
    agent_cwd: Option<String>,
    agent_context: Option<bool>,
    realtime_preview: Option<bool>,
    auto_paste: Option<bool>,
    active_pills: Option<Vec<String>>,
    ollama_url: Option<String>,
    ollama_model: Option<String>,
    chat_provider: Option<String>,
    chat_model: Option<String>,
    chat_base_url: Option<String>,
    claude_style: Option<String>,
    fish_voice: Option<String>,
    agent_voice: Option<bool>,
    popup_size: Option<String>,
    card_opacity: Option<f64>,
    popup_mochi: Option<bool>,
    popup_xy: Option<String>,
    accent_source: Option<String>,
    accent_custom: Option<String>,
) -> Result<(), String> {
    config::set(
        popup_pos,
        stt_model,
        theme,
        language,
        input_device,
        agent,
        agent_command,
        agent_cwd,
        agent_context,
        realtime_preview,
        auto_paste,
        active_pills,
        ollama_url,
        ollama_model,
        chat_provider,
        chat_model,
        chat_base_url,
        claude_style,
        fish_voice,
        agent_voice,
        popup_size,
        card_opacity,
        popup_mochi,
        popup_xy,
        accent_source,
        accent_custom,
    )
}

/// Agent session status for the settings card: `{active, agent, cwd,
/// lastPrompt, hasOutput}`. `active` = persistent session alive (tmux
/// `whisp-agent` or a terminal titled `whisp-agent`); `cwd` = folder the
/// next request would run from; `lastPrompt` = full context-carrying prompt
/// of the last Agent dictation, or null; `hasOutput` = the session pane
/// currently holds non-blank output (lets the frontend show something
/// useful without polling `agent_last_output` in a loop). Never fails:
/// degrades to inactives on missing state.
 #[tauri::command]
async fn agent_status(app: tauri::AppHandle) -> serde_json::Value {
    let last: Option<String> = app
         .try_state::<AppState>()
         .and_then(|s: State<'_, AppState>| s.agent_last.lock().ok().and_then(|g| g.clone()));
    // hyprctl + tmux ×2 + pgrep : jamais sur le thread principal, même avec
    // deadlines (un spawn lent suffit à geler les clics pendant le mount).
    let blocking = tauri::async_runtime::spawn_blocking(|| {
        (
            agent_session_active(),
            resolve_agent_cwd(),
            agent_pane_has_output(),
        )
    })
    .await
    .unwrap_or((false, config::agent_cwd_default(), false));
    let (active, cwd, has_output) = blocking;
    serde_json::json!({
        "active": active,
        "agent": config::agent_id(),
        "cwd": cwd,
        "lastPrompt": last,
        "hasOutput": has_output,
    })
}

/// True when `capture-pane` on the agent session returns non-blank text.
 /// Best effort (false on missing tmux/session): keeps `agent_status`
 /// infallible while sparing the frontend blind output polling.
 fn agent_pane_has_output() -> bool {
    // Deadline 5 s : pollé à 1 Hz en mode agent, un tmux coincé figeait tout.
    run_timed_out(&["tmux", "capture-pane", "-p", "-t", AGENT_SESSION], Duration::from_secs(5))
        .map(|o| o.chars().any(|c| !c.is_whitespace()))
        .unwrap_or(false)
 }
 
 /// Dernière sortie Agent pour le module inline de la popup (remplace l'ancienne island).
 /// `tmux capture-pane` sur `whisp-agent`, 20 dernières lignes, jamais de log.
 #[tauri::command]
async fn agent_last_output() -> Result<String, String> {
     // Deadline 5 s : même cause de freeze que ci-dessus (poll 1 Hz).
    // spawn_blocking en plus : même avec deadline, le spawn/wait ne doit
    // jamais toucher le thread principal.
    let text = tauri::async_runtime::spawn_blocking(|| {
        run_timed_out(
            &["tmux", "capture-pane", "-p", "-t", AGENT_SESSION, "-S", "-20"],
            Duration::from_secs(5),
        )
    })
    .await
    .map_err(|e| format!("agent output worker: {e}"))?
    .map_err(|_| {
        "Agent introuvable, vérifie Configuration (tmux absent ou session `whisp-agent` absente — dicte une demande pour la créer)".to_string()
    })?;
     let lines: Vec<&str> = text.lines().collect();
     let start = lines.len().saturating_sub(20);
     Ok(lines[start..].join("\n").chars().take(2000).collect())
}

/// Open (or focus) the agent terminal: focus `whisp-agent` when visible,
/// otherwise open a terminal with a shell in the default cwd. No agent is
/// launched; errors are returned as strings.
#[tauri::command]
fn agent_open_terminal() -> Result<(), String> {
    if agent_term_visible() {
        focus_agent_term();
        return Ok(());
    }
    if tmux_session_active() {
        spawn_agent_term(&format!(
            "exec tmux attach-session -t {AGENT_SESSION} 2>/dev/null || exec tmux new-session -s {AGENT_SESSION}"
        ));
        return Ok(());
    }
    if !(have_bin("foot") || have_bin("kitty") || have_bin("xterm")) {
        return Err("no terminal found (foot|kitty|xterm)".to_string());
    }
    let cwd = config::agent_cwd_default();
    let cwd_q = cwd.replace('\'', "'\\''");
    spawn_agent_term(&format!("cd '{cwd_q}' 2>/dev/null; exec sh"));
    Ok(())
}

/// Kill the running Agent request: `Ctrl-C` into the tmux session +
/// clear the busy flag so the next dictation can start. Visible depuis la
/// bouton Stop popup quand l'agent boucle. No-op when no live session
/// exists (Dictate-then-Stop without ever starting is not an error).
#[tauri::command]
fn agent_cancel(app: tauri::AppHandle) -> Result<(), String> {
    if tmux_session_active() {
        let _ = run_timed(
            &["tmux", "send-keys", "-t", AGENT_SESSION, "C-c"],
            TMUX_TIMEOUT,
        );
    }
    if let Some(state) = app.try_state::<AppState>() {
        state
            .agent_running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    match provider.as_str() {
        "fish_audio" => stt::set_fish_api_key(&key),
        "claude_oauth" => llm::set_claude_oauth_token(&key),
        "anthropic" | "openai" | "google" | "openrouter" | "deepseek" | "qwen" | "moonshot" => {
            llm::set_cloud_key(&provider, &key)
        }
        _ => Err(format!("unknown provider '{provider}'")),
    }
}

#[tauri::command]
fn has_api_key(provider: String) -> bool {
    if provider == "claude" || provider == "claude_oauth" {
        return llm::claude_oauth_status();
    }
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
/// Connexion Claude abonnement : `{connected, has_style}` — jamais le token.
#[tauri::command]
fn claude_status() -> serde_json::Value {
    let style = crate::config::load()
        .claude_style
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    serde_json::json!({
        "connected": llm::claude_oauth_status(),
        "has_style": style.is_some(),
        "style_len": style.map(|s| s.len()).unwrap_or(0),
    })
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
        check(
            "claude",
            "Claude Code",
            home.join(".claude/settings.json"),
            "whisp-hook",
        ),
        check("codex", "Codex", home.join(".codex/config.toml"), "whisp"),
        check(
            "gemini",
            "Gemini CLI",
            home.join(".gemini/settings.json"),
            "whisp-hook",
        ),
        check(
            "copilot",
            "Copilot CLI",
            home.join(".config/github-copilot/hooks.json"),
            "whisp",
        ),
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
    // Deadline 120 s : le script touche le FS + réseau (npm), jamais infini.
    // run_timed_out jette stderr : en échec on relance pas, on dégrade le
    // message (le détail était sur stderr, rarement utile).
    let out = run_timed_out(
        &["bash", &script.to_string_lossy(), "--agent", &agent],
        Duration::from_secs(120),
    )
    .map_err(|e| format!("install-hook: {e}"))?;
    let msg = out.trim().to_string();
    if msg.is_empty() {
        return Err("install-hook: échec sans message (voir logs)".to_string());
    }
    Ok(msg)
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
    let n = tags
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(format!("Ollama OK ({base}, {n} modèle(s))"))
}

 #[tauri::command]
async fn list_input_devices() -> Vec<String> {
    // Énumération ALSA (jusqu'à 5 s sur certains systèmes) : jamais sur le
    // thread principal — le mount Settings gelait les clics en attendant.
    tauri::async_runtime::spawn_blocking(audio::list_input_devices)
        .await
        .unwrap_or_default()
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
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &sep,
            &record,
            &cycle,
            &history_item,
            &settings,
            &sep,
            &quit,
        ],
    )
    .map_err(|e| e.to_string())?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("whisp — clic pour ouvrir")
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;
            // Left click = open the main window (Wayland appindicator
            // menus don't always open on click).
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    ..
                }
            ) {
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
    // Debounce toggle : Hyprland bind + global-shortcut Tauri tirent parfois
    // en double (ferme → rouvre → referme). Un seul toggle / 800 ms.
    static LAST_TOGGLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if id == "tray-record" {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if now.wrapping_sub(LAST_TOGGLE.load(std::sync::atomic::Ordering::SeqCst)) < 800 {
            return;
        }
        LAST_TOGGLE.store(now, std::sync::atomic::Ordering::SeqCst);
    }
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
                    let mode = modes::get_mode(&mode_id)
                        .unwrap_or_else(|| modes::builtin_modes().into_iter().next().unwrap());
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
        emit_state(
            &handler_app,
            RecStatus::Error,
            Some(format!("hotkeys: {e}")),
        );
        eprintln!("[whisp] hotkeys: {e}");
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
            // $XDG_RUNTIME_DIR d'abord (0600 implicite) : /tmp prédictible = risque.
            let sock_name = format!(
                "whisp-{}.sock",
                std::env::var("UID")
                    .ok()
                    .and_then(|u| u.parse::<u32>().ok())
                    .unwrap_or(1000)
            );
            let path = std::env::var("XDG_RUNTIME_DIR")
                .map(std::path::PathBuf::from)
                .ok()
                .filter(|d: &std::path::PathBuf| d.is_dir())
                .map(|d| d.join(&sock_name))
                .unwrap_or(std::env::temp_dir().join(&sock_name));
            let _ = std::fs::remove_file(&path);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Chmod 0600 : seul l'utilisateur parle au control plane.
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&path)
                    .and_then(|f| {
                        f.set_permissions(std::fs::Permissions::from_mode(0o600))
                            .map(|_| f)
                    });
                let _ = std::fs::remove_file(&path);
            }
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
/// connection per call), reply, close.
///
/// Timeouts (aucun thread parké) : read 10 s (une ligne JSON arrive en ms ;
/// au-delà c'est un peer mort), write 130 s (couvre le `chat` bloquant à
/// 120 s + marge ; `stop` répond de suite, son pipeline est async).
/// `MAX_CMD_LEN` 8 Ko : pas de prompt-sized pastes sur le control plane.
const SOCKET_READ_TIMEOUT: Duration = Duration::from_secs(10);
const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(130);
fn handle_socket_conn(app: &tauri::AppHandle, stream: std::os::unix::net::UnixStream) {
    use std::io::{BufRead, BufReader, Write};
    let _ = stream.set_read_timeout(Some(SOCKET_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_WRITE_TIMEOUT));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    // Bound the line: no prompt-sized pastes on the control plane.
    const MAX_CMD_LEN: usize = 8192;
    if line.len() > MAX_CMD_LEN {
        let fail = serde_json::json!({"ok": false, "error": "command too long"}).to_string() + "\n";
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
    let ok = |data: serde_json::Value| serde_json::json!({"ok": true, "data": data}).to_string();
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
            let handle: Option<audio::RecordingHandle> =
                app.try_state::<AppState>()
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
            let mode = modes::get_mode(&mode_id)
                .unwrap_or_else(|| modes::builtin_modes().into_iter().next().unwrap());
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
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
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
        // Chat via socket (MCP) : provider + messages [{role, content}].
        // Bloquant côté thread connexion (comme `history`), timeout 130 s.
        "chat" => {
            let provider = v
                .get("provider")
                .and_then(|p| p.as_str())
                .unwrap_or("claude")
                .to_string();
            let msgs: Vec<llm::ChatMessage> = v
                .get("messages")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let role = m.get("role")?.as_str()?.to_string();
                            let content = m.get("content")?.as_str()?.to_string();
                            Some(llm::ChatMessage { role, content })
                        })
                        .collect()
                })
                .unwrap_or_default();
            if msgs.is_empty() {
                return fail("empty conversation".to_string());
            }
            match llm::chat(&provider, &msgs) {
                Ok(reply) => ok(serde_json::json!({ "provider": provider, "reply": reply })),
                Err(e) => fail(e),
            }
        }
        "set_config" => {
            let str_field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
            match config::set(
                str_field("popupPos"),
                str_field("sttModel"),
                str_field("theme"),
                str_field("language"),
                str_field("inputDevice"),
                str_field("agent"),
                str_field("agentCommand"),
                str_field("agentCwd"),
                v.get("agentContext").and_then(|x| x.as_bool()),
                v.get("realtimePreview").and_then(|x| x.as_bool()),
                v.get("autoPaste").and_then(|x| x.as_bool()),
                v.get("activePills").and_then(|x| x.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|y| y.as_str().map(|s| s.to_string()))
                        .collect::<Vec<String>>()
                }),
                str_field("ollamaUrl"),
                str_field("ollamaModel"),
                str_field("chatProvider"),
                str_field("chatModel"),
                str_field("chatBaseUrl"),
                str_field("claudeStyle"),
                str_field("fishVoice"),
                v.get("agentVoice").and_then(|x| x.as_bool()),
                str_field("popupSize"),
                v.get("cardOpacity").and_then(|x| x.as_f64()),
                v.get("popupMochi").and_then(|x| x.as_bool()),
                str_field("popupXy"),
                str_field("accentSource"),
                str_field("accentCustom"),
            ) {
                Ok(()) => ok(serde_json::json!(config::load())),
                Err(e) => fail(e),
            }
        }
        "fit" => {
            let w = v.get("w").and_then(|x| x.as_f64()).unwrap_or(576.0);
            let h = v.get("h").and_then(|x| x.as_f64()).unwrap_or(400.0);
            match fit_window(app.clone(), w, h) {
                Ok(m) => ok(serde_json::json!({"fitted": m})),
                Err(e) => fail(e),
            }
        }
        "set_stt" => {
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let stt = v
                .get("stt")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        .manage(agent_hook::PendingApprovals::default())
        .setup(|app| {
        register_hotkeys(app.handle());
        if build_tray(app.handle()).is_err() {
        // Best effort: tray is convenience, recorder works without it.
        }
        spawn_toggle_socket(app.handle().clone());
        agent_hook::spawn_agents_socket(app.handle().clone());
        agent_hook::ensure_relay_installed();
        spawn_main_thread_watchdog(app.handle().clone());
        // Recorder starts hidden (see tauri.conf.json `visible: false`).
        Ok(())
        })
        .on_menu_event(|app, event| on_tray_event(app, event.id.as_ref()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Fermer = cacher (le tray permet de rouvrir).
                // Seul "Quitter" du tray arrête vraiment l'app.
                api.prevent_close();
                let _ = window.hide();
            }
            if matches!(event, tauri::WindowEvent::Destroyed) {
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
            delete_history,
            get_config,
            set_config,
            claude_status,
            set_api_key,
            agent_status,
            agent_open_terminal,
            agent_cancel,
            agent_last_output,
            fit_window,
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
            caelestia::caelestia_scheme,
            caelestia::caelestia_watch,
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

#[cfg(test)]
mod agent_tests {
    use super::*;

    #[test]
    fn inner_command_quotes_apostrophes() {
        let cmd = agent_inner_command("omp", "", "dis l'amour s'il te plaît");
        assert_eq!(cmd, "omp -p 'dis l'\\''amour s'\\''il te plaît'");
    }

    #[test]
    fn inner_command_custom_template() {
        let cmd = agent_inner_command("omp", "mywrap --task {prompt} --yes", "hello world");
        assert_eq!(cmd, "mywrap --task 'hello world' --yes");
    }

    #[test]
    fn inner_command_preset_or_fallback_is_single_line() {
        // `codex` may or may not be installed on the test machine: accept
        // either the preset or the omp fallback, but never a raw newline.
        let cmd = agent_inner_command("codex", "", "hello\n\tworld");
        assert!(
            cmd == "codex exec 'hello world'" || cmd == "omp -p 'hello world'",
            "unexpected: {cmd}"
        );
    }

    #[test]
    fn preset_bin_falls_back_when_binary_missing() {
        assert_eq!(preset_agent_bin("claude", |_| false), "omp");
        assert_eq!(preset_agent_bin("codex", |_| false), "omp");
        assert_eq!(preset_agent_bin("claude", |b| b == "claude"), "claude");
        assert_eq!(preset_agent_bin("codex", |_| true), "codex");
        assert_eq!(preset_agent_bin("omp", |_| true), "omp");
        assert_eq!(preset_agent_bin("weird", |_| true), "omp");
    }

    #[test]
    fn prompt_truncates_long_requests_on_char_boundary() {
        let out = agent_prompt_inner(&"é".repeat(5000), "/home/u/Projets/whisp", None, true);
        assert!(out.contains(&format!("Demande: {}", "é".repeat(AGENT_PROMPT_MAX_CHARS))));
        assert!(!out.contains(&"é".repeat(AGENT_PROMPT_MAX_CHARS + 1)));
    }

    #[test]
    fn prompt_drops_missing_screenshot_and_names_folder() {
        let out = agent_prompt_inner(
            "fais ça",
            "/home/u/Projets/whisp",
            Some("/nonexistent-whisp-shot-xyz.png"),
            true,
        );
        assert!(!out.contains("Capture"), "lying screenshot: {out}");
        assert!(
            out.contains("whisp (/home/u/Projets/whisp)"),
            "basename missing: {out}"
        );
        assert!(out.ends_with("Demande: fais ça"), "tail: {out}");
    }

    #[test]
    fn prompt_keeps_existing_screenshot() {
        let path = std::env::temp_dir().join("whisp-agent-test-shot.png");
        std::fs::write(&path, b"fake-png").unwrap();
        let out = agent_prompt_inner("go", "/tmp", Some(&path.to_string_lossy()), true);
        let _ = std::fs::remove_file(&path);
        assert!(out.contains("Capture d'écran:"), "shot dropped: {out}");
        assert!(
            out.contains(path.to_str().unwrap()),
            "shot path missing: {out}"
        );
    }

    #[test]
    fn prompt_plain_without_context() {
        let out = agent_prompt_inner("hello world", "/x", Some("/nope.png"), false);
        assert_eq!(out, "hello world");
    }
}
