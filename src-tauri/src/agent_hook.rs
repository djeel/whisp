//! whisp agent-hook plane: Unix socket server + shared normalization.
//!
//! Agents (Claude Code, Gemini CLI, Copilot CLI, …) report lifecycle events by
//! piping their hook JSON into the `whisp-hook` relay binary, which forwards
//! one JSON object per line (`\n`-terminated, max 1 MiB) to
//! `$XDG_RUNTIME_DIR/whisp-agents.sock`
//! (fallback `/tmp/whisp-agents-<uid>.sock` when `XDG_RUNTIME_DIR` is unset).
//! This module serves that socket inside the Tauri app and re-emits each
//! normalized payload to the frontend as the `whisp://agent` event.
//!
//! Canonical agents: events whose `hook_event_name` is already canonical need
//! no `--agent` flag. Anything else is tagged via `whisp-hook --agent <nom>`
//! (validated `^[a-z0-9-]{1,24}$`) and its event names pass through unchanged.
//!
//! Claude Code wiring (`~/.claude/settings.json`):
//! ```json
//! {
//!   "hooks": {
//!     "UserPromptSubmit": [{ "hooks": [{ "type": "command",
//!       "command": "$HOME/.local/share/whisp/bin/whisp-hook" }] }],
//!     "PreToolUse":  [{ "hooks": [{ "type": "command",
//!       "command": "$HOME/.local/share/whisp/bin/whisp-hook" }] }],
//!     "PostToolUse": [{ "hooks": [{ "type": "command",
//!       "command": "$HOME/.local/share/whisp/bin/whisp-hook" }] }],
//!     "Stop":        [{ "hooks": [{ "type": "command",
//!       "command": "$HOME/.local/share/whisp/bin/whisp-hook" }] }]
//!   }
//! }
//! ```
//! Non-Claude agents add `--agent <nom>` before the event name, e.g.
//! `whisp-hook --agent gemini-cli`.
//!
//! Rules: never block the calling agent (short budgets, fire-and-forget except
//! `PermissionRequest`), never log prompt text in clear.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::{Emitter, Manager};

/// Canonical hook events. Unknown names pass through untouched.
/// (Reference for hook authors; the live path tolerates any event name.)
#[allow(dead_code)]
pub const CANONICAL_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
    "PermissionRequest",
];

/// Fields that are pointless to forward and can be enormous.
const DROPPED_FIELDS: &[&str] = &["tool_response", "tool_output", "transcript_path"];
/// Longest string forwarded for any single field.
const MAX_FIELD_LEN: usize = 2_000;
/// Longest line (with `\n`) read from the socket or stdin.
const MAX_LINE_LEN: usize = 1024 * 1024;

/// `$XDG_RUNTIME_DIR/whisp-agents.sock`, or `/tmp/whisp-agents-<uid>.sock`.
pub fn agents_socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return dir.join("whisp-agents.sock");
    }
    #[cfg(target_os = "linux")]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(target_os = "linux"))]
    let uid = 1000u32;
    PathBuf::from(format!("/tmp/whisp-agents-{uid}.sock"))
}

/// Canonical event name for `name`; unknown names pass through.
pub fn normalize_event(name: &str) -> &str {
    match name {
        // Gemini CLI
        "BeforeTool" | "BeforeToolSelection" => "PreToolUse",
        "AfterTool" | "AfterModel" => "PostToolUse",
        "BeforeAgent" => "UserPromptSubmit",
        "AfterAgent" => "Stop",
        "startup" => "SessionStart",
        "exit" => "SessionEnd",
        // Antigravity
        "PreInvocation" => "UserPromptSubmit",
        "PostInvocation" => "PostToolUse",
        // snake_case relays
        "pre_tool_use" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "user_prompt_submit" => "UserPromptSubmit",
        "session_start" => "SessionStart",
        "session_end" => "SessionEnd",
        "stop" => "Stop",
        // Copilot CLI (camelCase)
        "sessionStart" => "SessionStart",
        "userPromptSubmitted" => "UserPromptSubmit",
        "agentStop" => "Stop",
        "notification" => "Notification",
        "preToolUse" => "PreToolUse",
        "postToolUse" => "PostToolUse",
        "permissionRequest" => "PermissionRequest",
        "sessionEnd" => "SessionEnd",
        // Cursor
        "beforeSubmitPrompt" => "UserPromptSubmit",
        "postToolUseFailure" => "PostToolUseFailure",
        other => other,
    }
}

/// `^[a-z0-9-]{1,24}$`, ASCII only.
pub fn valid_agent(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 24
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One event, ready to forward (`line` already ends with `\n`).
/// (Kept for unit tests / future relay unification.)
#[allow(dead_code)]
pub struct Prepared {
    pub line: String,
    pub name: String,
}

/// Build the forward payload from raw stdin bytes. `None` = nothing forwardable.
/// (Kept for unit tests; the installed relay inlines the same logic so the
/// binary only depends on serde_json + libc.)
#[allow(dead_code)]
pub fn prepare_payload(
    raw: &[u8],
    agent_arg: &str,
    event_arg: &str,
    cwd: &str,
) -> Option<Prepared> {
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let mut payload = serde_json::from_slice::<Value>(raw).ok()?;
    let map = payload.as_object_mut()?;

    if valid_agent(agent_arg) {
        map.insert("coucou_agent".into(), Value::String(agent_arg.to_string()));
    }
    fill_tool_fields(map);

    let raw_event = map
        .get("hook_event_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(event_arg);
    let name = refine(normalize_event(raw_event), map);
    map.insert("hook_event_name".into(), Value::String(name.clone()));

    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    if map
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::is_empty)
        .unwrap_or(true)
        && !cwd.is_empty()
    {
        map.insert("cwd".into(), Value::String(cwd.to_string()));
    }
    truncate_strings(&mut payload);

    let mut line = payload.to_string();
    line.push('\n');
    Some(Prepared { line, name })
}

/// Fill `tool_name` / `tool_input` / `session_id` from the aliases other
/// agents use. Never overwrites a field the payload already has.
fn fill_tool_fields(map: &mut Map<String, Value>) {
    let call = map.get("toolCall").and_then(Value::as_object).cloned();
    let Some(call) = call else {
        return;
    };
    if !map.contains_key("tool_name") {
        for k in ["toolName", "name"] {
            if let Some(s) = call
                .get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                map.insert("tool_name".into(), Value::String(s.to_string()));
                break;
            }
        }
    }
    if !map.contains_key("tool_input") {
        for k in ["args", "input", "parameters"] {
            if let Some(v) = call.get(k) {
                if !v.is_null() {
                    map.insert("tool_input".into(), v.clone());
                    break;
                }
            }
        }
    }
    if map
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::is_empty)
        .unwrap_or(true)
    {
        for k in ["sessionId", "sessionID"] {
            if let Some(s) = map.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                map.insert("session_id".into(), Value::String(s.to_string()));
                break;
            }
        }
    }
}

/// A stop/post-tool-use reporting an error becomes its `Failure` variant.
fn refine(event: &str, map: &Map<String, Value>) -> String {
    let failed = map
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("error"))
        || map.get("error").is_some_and(|v| !v.is_null())
        || map
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if failed {
        match event {
            "PostToolUse" => return "PostToolUseFailure".to_string(),
            "Stop" => return "StopFailure".to_string(),
            _ => {}
        }
    }
    event.to_string()
}

/// Cap every string in the payload; a single Write can carry a whole file.
fn truncate_strings(value: &mut Value) {
    match value {
        Value::String(s) => cut(s, MAX_FIELD_LEN),
        Value::Array(items) => items.iter_mut().for_each(truncate_strings),
        Value::Object(map) => map.values_mut().for_each(truncate_strings),
        _ => {}
    }
}

fn cut(s: &mut String, max: usize) {
    if s.len() > max {
        s.truncate(max);
        s.push('…');
    }
}

/// Normalize a socket line for the frontend: ensure `event`, `agent`,
/// `session_id`. Returns `None` for non-object / unparseable lines.
pub fn normalize_for_front(line: &str) -> Option<Value> {
    let mut payload = serde_json::from_str::<Value>(line).ok()?;
    let map = payload.as_object_mut()?;
    let raw_event = map
        .get("hook_event_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| map.get("event").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let name = refine(normalize_event(&raw_event), map);
    map.insert("hook_event_name".into(), Value::String(name.clone()));
    map.insert("event".into(), Value::String(name));
    let agent = ["whisp_agent", "coucou_agent", "agent"]
        .iter()
        .filter_map(|k| {
            map.get(*k)
                .and_then(Value::as_str)
                .filter(|s| valid_agent(s))
        })
        .next()
        .map(str::to_string)
        .unwrap_or_else(|| "agent".to_string());
    map.insert("agent".into(), Value::String(agent));
    if map.get("session_id").and_then(Value::as_str).is_none() {
        map.insert("session_id".into(), Value::String(String::new()));
    }
    Some(payload)
}
/// Single forward point for socket lines: event vers le module Agent inline
/// de la popup (`whisp://agent` + `whisp://agent-event`).
fn forward_to_front(app: &tauri::AppHandle, payload: &Value) {
    let _ = app.emit("whisp://agent", payload);
    let _ = app.emit("whisp://agent-event", payload);
}

/// Slightly under the relay's own 110 s wait, so the app always answers first.
const DECISION_WAIT: std::time::Duration = std::time::Duration::from_secs(108);
/// How long the island gets to say "the card is up". Without it, an island
/// that is paused or not listening would hold the agent for two minutes.
const ACK_WAIT: std::time::Duration = std::time::Duration::from_millis(800);

/// What the island can say about a permission request.
#[derive(Debug)]
enum Reply {
    /// The card is on screen and a human can act on it.
    Ack,
    /// A human clicked: `allow`/`always`/`deny`, or `{"answers":{…}}`.
    Decision(String),
    /// Nobody can act on it (paused, queue full, unknown agent): terminal takes over.
    Decline,
}

/// Permission requests waiting on a human click, by `request_id`.
pub struct PendingApprovals(
    pub std::sync::Mutex<std::collections::HashMap<String, std::sync::mpsc::Sender<Reply>>>,
);

impl Default for PendingApprovals {
    fn default() -> Self {
        Self(std::sync::Mutex::new(std::collections::HashMap::new()))
    }
}

/// Agents whose permission requests get a card. MUST match `takes_decisions`
/// in the relay (`src/bin/whisp-hook.rs`): any other agent is declined at
/// once, silently, so it asks in its own terminal.
pub fn takes_decisions(agent: &str) -> bool {
    matches!(agent, "" | "codex" | "copilot" | "muse" | "claude-desktop")
}
/// Serve `$XDG_RUNTIME_DIR/whisp-agents.sock`: one JSON object per line,
pub fn spawn_agents_socket(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("whisp-agents-sock".to_string())
        .spawn(move || {
            use std::os::unix::net::{UnixListener, UnixStream};
            let path = agents_socket_path();
            // Stale socket: a live peer means another instance serves it.
            if UnixStream::connect(&path).is_ok() {
                return;
            }
            let _ = std::fs::remove_file(&path);
            let listener = match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("whisp-agents: bind failed: {e}");
                    return;
                }
            };
            for stream in listener.incoming().flatten() {
                if !same_user(&stream) {
                    continue;
                }
                // One thread per connection: a PermissionRequest waits up to
                // ~108 s for a human click while other agents keep reporting.
                let app = app.clone();
                std::thread::Builder::new()
                    .name("whisp-agents-conn".to_string())
                    .spawn(move || handle_connection(&app, stream))
                    .ok();
            }
        })
        .ok();
}

/// One relay connection: read lines until it goes quiet. Each line is one
/// hook event; a `PermissionRequest` keeps its connection open while the
/// island waits for a human click, and the decision goes back on the same
/// socket. Closing without a reply means "no decision": the relay writes
/// nothing and the agent asks in its own terminal.
fn handle_connection(app: &tauri::AppHandle, stream: std::os::unix::net::UnixStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        if line.len() > MAX_LINE_LEN {
            continue;
        }
        match normalize_for_front(line.trim()) {
            Some(payload) => {
                // Metadata only: never log prompt/tool text.
                eprintln!(
                    "whisp-agents: event={} agent={} session={}",
                    payload.get("event").and_then(Value::as_str).unwrap_or("?"),
                    payload.get("agent").and_then(Value::as_str).unwrap_or("?"),
                    payload
                        .get("session_id")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                if payload.get("event").and_then(Value::as_str) == Some("PermissionRequest") {
                    handle_permission(app, reader.get_mut(), payload);
                    break; // one decision per connection; the relay opens a fresh one next time.
                }
                forward_to_front(app, &payload);
            }
            None => eprintln!("whisp-agents: dropped malformed line"),
        }
    }
}

/// Short id for one waiting request (relay pid + counter: unique per app run).
fn next_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// A `PermissionRequest`: give the island a card, ACK fast, then wait for the
/// human click. Anything but an explicit decision stays silent on the socket.
fn handle_permission(
    app: &tauri::AppHandle,
    stream: &mut std::os::unix::net::UnixStream,
    mut payload: Value,
) {
    use std::io::Write;
    let agent = payload
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("agent");
    // Agents whose decisions the island cannot give: decline at once, silently,
    // so the agent asks in its own terminal (documented in takes_decisions).
    if !takes_decisions(agent) {
        forward_to_front(app, &payload);
        return;
    }
    let id = next_request_id();
    let (tx, rx) = std::sync::mpsc::channel::<Reply>();
    {
        let pending = app.try_state::<PendingApprovals>();
        if let Some(pending) = pending {
            pending.0.lock().map(|mut m| m.insert(id.clone(), tx)).ok();
        }
    }
    payload["request_id"] = Value::String(id.clone());
    forward_to_front(app, &payload);
    // Plus d'island : ACK direct, l'agent demande dans son terminal.
    // (Les PermissionRequest avec décision inline arriveront via agent_last_output.)
    send_reply(app, &id, Reply::Ack, true);
    let decision = wait_for_decision(&rx);
    app.try_state::<PendingApprovals>()
        .map(|p| p.0.lock().map(|mut m| m.remove(&id)).ok());
    match decision {
        Some(d) => {
            let mut line = d;
            line.push('\n');
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
            let _ = stream.write_all(line.as_bytes());
            let _ = stream.flush();
        }
        // No decision: close without a reply. The relay then writes nothing
        // and the agent asks in its terminal, exactly as if whisp were closed.
        None => {}
    }
}

/// Two waits: a short one for "the card is up", then the long one for a human.
/// A click that beats the ack is still a click.
fn wait_for_decision(rx: &std::sync::mpsc::Receiver<Reply>) -> Option<String> {
    match rx.recv_timeout(ACK_WAIT) {
        Ok(Reply::Ack) => {}
        Ok(Reply::Decision(d)) => return Some(d),
        Ok(Reply::Decline) => return None,
        Err(_) => return None,
    }
    match rx.recv_timeout(DECISION_WAIT) {
        Ok(Reply::Decision(d)) => Some(d),
        _ => None,
    }
}

/// Deliver the island's word on one request. `keep` holds the slot for the
/// later decision (ack); otherwise the slot closes (decision/decline).
fn send_reply(app: &tauri::AppHandle, request_id: &str, reply: Reply, keep: bool) {
    let sender = app.try_state::<PendingApprovals>().and_then(|pending| {
        pending.0.lock().ok().and_then(|mut m| {
            if keep {
                m.get(request_id).cloned()
            } else {
                m.remove(request_id)
            }
        })
    });
    match sender {
        Some(tx) => {
            let _ = tx.send(reply);
        }
        None => eprintln!("whisp-agents: reply for id={request_id} — no pending request"),
    }
}

/// Called by the island's Allow / Deny buttons. Only ever a bare word:
/// turning it into hookSpecificOutput JSON is the relay's job.
pub fn answer(app: &tauri::AppHandle, request_id: &str, decision: &str) {
    let word = match decision {
        "allow" | "always" => "allow",
        _ => "deny",
    };
    send_reply(app, request_id, Reply::Decision(word.to_string()), false);
}

/// Called when an option is picked for a question the agent asked.
/// `answers` maps each question text to its picked label(s).
pub fn answer_question(
    app: &tauri::AppHandle,
    request_id: &str,
    answers: &std::collections::HashMap<String, Value>,
) {
    // One line: the relay reads up to the first newline.
    let line = serde_json::json!({ "answers": answers }).to_string();
    send_reply(app, request_id, Reply::Decision(line), false);
}

/// Drop one request without a decision (queue full, expired, superseded):
/// the agent asks in its own terminal.
pub fn decline(app: &tauri::AppHandle, request_id: &str) {
    send_reply(app, request_id, Reply::Decline, false);
}

/// True when the peer process runs as us (SO_PEERCRED). Closed on doubt.
#[cfg(target_os = "linux")]
fn same_user(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast::<libc::c_void>(),
            &mut len,
        )
    };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

#[cfg(not(target_os = "linux"))]
fn same_user(_stream: &std::os::unix::net::UnixStream) -> bool {
    true
}

/// Copy the relay to `~/.local/share/whisp/bin/whisp-hook` when a bundled
/// copy is available next to the app binary and the destination is missing or
/// stale. Best effort: never fails startup.
pub fn ensure_relay_installed() {
    let dest = match dirs::data_dir() {
        Some(d) => d.join("whisp/bin/whisp-hook"),
        None => return,
    };
    let src = match std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
    {
        Some(dir) => dir.join("whisp-hook"),
        None => return,
    };
    let fresh = match (std::fs::metadata(&src), std::fs::metadata(&dest)) {
        (Ok(s), Ok(d)) => s.len() == d.len(),
        (Ok(_), Err(_)) => false,
        _ => return,
    };
    if fresh {
        return;
    }
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::copy(&src, &dest).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
        }
    }
}
