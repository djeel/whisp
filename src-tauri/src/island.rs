//! Agent island: session pills + auto show/hide for third-party agents.
//!
//! Events arrive from the agents socket (one JSON object per line, forwarded
//! by the relay) via [`handle_event`]. Each `(agent, session_id)` pair owns a
//! pill; state follows the coucou lifecycle (SessionStart → idle,
//! UserPromptSubmit → thinking, PreToolUse → working, Stop → finished, …).
//! The frontend listens on `whisp://agent` (`{"pills": [...]}`) and pulls the
//! same snapshot via the `island_list` command.
//!
//! Never blocks the caller: only short mutexes + one emit. Never logs prompt
//! text: payload content stays in memory and goes to the UI only.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Idle delay after which the island window hides itself.
const IDLE_HIDE: Duration = Duration::from_secs(60);
/// Ticker shows a single line, capped here.
const MAX_LINE: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PillState {
    Idle,
    Working,
    Thinking,
    Finished,
    Error,
}

/// One question Claude Code asked via `AskUserQuestion`: the text, the option
/// labels the user may pick, and whether several may be picked at once.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub multi: bool,
}

/// A permission request waiting for a human click. Shown as a card; answered
/// only via `agent_decide` (never auto-allowed). `questions` is `Some` when
/// the agent asked a question instead of asking for a permission.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalCard {
    pub request_id: String,
    pub session_id: String,
    pub agent: String,
    pub tool: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<ApprovalQuestion>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pill {
    pub session_id: String,
    pub agent: String,
    pub state: PillState,
    pub last_line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub pending_approval: bool,
    pub updated_at: u64,
}

pub struct IslandState {
    pills: Mutex<HashMap<String, Pill>>,
    last_activity: Mutex<Option<Instant>>,
    approvals: Mutex<Vec<ApprovalCard>>,
}

impl IslandState {
    pub fn new() -> Self {
        Self {
            pills: Mutex::new(HashMap::new()),
            last_activity: Mutex::new(None),
            approvals: Mutex::new(Vec::new()),
        }
    }

    fn snapshot(&self) -> Vec<Pill> {
        let mut v: Vec<Pill> = self
            .pills
            .lock()
            .map(|g| g.values().cloned().collect())
            .unwrap_or_default();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        v
    }

    fn touch(&self) {
        if let Ok(mut g) = self.last_activity.lock() {
            *g = Some(Instant::now());
        }
    }
}

/// Most permission cards waiting at once, the one on screen included.
pub const MAX_APPROVALS: usize = 5;

impl IslandState {
    /// Current cards, oldest first.
    pub fn approvals_snapshot(&self) -> Vec<ApprovalCard> {
        self.approvals.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Queue a permission card. `Err` on duplicate or full queue: the caller then
/// stays silent and the agent asks in its own terminal. Never blocks, never
/// logs content.
pub fn push_approval(app: &AppHandle, card: ApprovalCard) -> Result<(), &'static str> {
    let Some(state) = app.try_state::<IslandState>() else {
        return Err("no state");
    };
    {
        let mut approvals = state.approvals.lock().map_err(|_| "locked")?;
        if approvals.iter().any(|a| a.request_id == card.request_id) {
            return Err("duplicate");
        }
        if approvals.len() >= MAX_APPROVALS {
            return Err("full");
        }
        approvals.push(card);
    }
    state.touch();
    emit_full(app);
    show_island(app);
    Ok(())
}

/// Drop the card for `request_id` (answered, expired or superseded) and clear
/// the pill badge when no card for that session remains. True when removed.
pub fn resolve_approval(app: &AppHandle, request_id: &str) -> bool {
    let Some(state) = app.try_state::<IslandState>() else {
        return false;
    };
    let rest = {
        let mut approvals = match state.approvals.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let before = approvals.len();
        approvals.retain(|a| a.request_id != request_id);
        if approvals.len() == before {
            return false;
        }
        approvals.clone()
    };
    if let Ok(mut pills) = state.pills.lock() {
        for pill in pills.values_mut() {
            let alive = rest
                .iter()
                .any(|a| a.agent == pill.agent && a.session_id == pill.session_id);
            if !alive && pill.pending_approval {
                pill.pending_approval = false;
                pill.updated_at = now_secs();
            }
        }
    }
    emit_full(app);
    true
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Agent tag validation: `^[a-z0-9-]{1,24}$`, same rule as coucou.
fn valid_agent(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 24
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Canonical event name. Unknown names pass through as `"Other"` (pill keeps
/// its state, ticker still refreshes when the payload carries text).
fn canonical(name: &str) -> &str {
    match name {
        // Claude Code / canonical
        "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse"
        | "PostToolUseFailure" | "Notification" | "Stop" | "StopFailure" | "SessionEnd"
        | "SubagentStart" | "SubagentStop" | "PermissionRequest" => name,
        // Gemini CLI
        "BeforeTool" | "BeforeToolSelection" => "PreToolUse",
        "AfterTool" | "AfterModel" => "PostToolUse",
        "BeforeAgent" | "PreInvocation" => "UserPromptSubmit",
        "AfterAgent" => "Stop",
        "startup" => "SessionStart",
        "exit" => "SessionEnd",
        // snake_case relays
        "pre_tool_use" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "user_prompt_submit" => "UserPromptSubmit",
        "session_start" => "SessionStart",
        "session_end" => "SessionEnd",
        "stop" => "Stop",
        // Copilot CLI / Cursor (camelCase)
        "sessionStart" => "SessionStart",
        "userPromptSubmitted" | "beforeSubmitPrompt" => "UserPromptSubmit",
        "agentStop" => "Stop",
        "notification" => "Notification",
        "preToolUse" => "PreToolUse",
        "postToolUse" => "PostToolUse",
        "postToolUseFailure" => "PostToolUseFailure",
        "permissionRequest" => "PermissionRequest",
        "sessionEnd" => "SessionEnd",
        "subagentStart" => "SubagentStart",
        "subagentStop" => "SubagentStop",
        _ => "Other",
    }
}

fn str_field<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.trim().is_empty())
}

/// First line of `s`, trimmed, capped at [`MAX_LINE`] chars.
fn short(s: &str) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    if line.chars().count() <= MAX_LINE {
        line.to_string()
    } else {
        let mut t: String = line.chars().take(MAX_LINE).collect();
        t.push('…');
        t
    }
}

/// Human label for a tool event: `tool_name` + the most useful detail field.
fn tool_label(v: &serde_json::Value) -> String {
    let tool = str_field(v, "tool_name").unwrap_or("tool");
    let detail = ["command", "file_path", "path", "query", "pattern", "url"]
        .iter()
        .filter_map(|k| str_field(v, k))
        .next()
        .map(short)
        .unwrap_or_default();
    if detail.is_empty() {
        tool.to_string()
    } else {
        format!("{tool} {detail}")
    }
}

fn ticker_line(event: &str, v: &serde_json::Value) -> String {
    match event {
        "UserPromptSubmit" => ["prompt", "text", "message"]
            .iter()
            .filter_map(|k| str_field(v, k))
            .next()
            .map(short)
            .unwrap_or_default(),
        "PreToolUse" => short(&tool_label(v)),
        "PermissionRequest" => {
            let tool = str_field(v, "tool_name").unwrap_or("permission");
            short(&format!("{tool} — approval?"))
        }
        "PostToolUse" => short(&format!("{} done", str_field(v, "tool_name").unwrap_or("tool"))),
        "PostToolUseFailure" => {
            short(&format!("{} failed", str_field(v, "tool_name").unwrap_or("tool")))
        }
        "Notification" => ["message", "notification", "text"]
            .iter()
            .filter_map(|k| str_field(v, k))
            .next()
            .map(short)
            .unwrap_or_default(),
        "SubagentStart" => short("subagent started"),
        "SubagentStop" => short("subagent done"),
        "Stop" => ["result", "summary", "text"]
            .iter()
            .filter_map(|k| str_field(v, k))
            .next()
            .map(short)
            .unwrap_or_default(),
        _ => ["text", "message", "prompt", "command"]
            .iter()
            .filter_map(|k| str_field(v, k))
            .next()
            .map(short)
            .unwrap_or_default(),
    }
}

fn emit_full(app: &AppHandle) {
    let Some(state) = app.try_state::<IslandState>() else {
        return;
    };
    let pills = state.snapshot();
    let approvals = state.approvals_snapshot();
    let _ = app.emit(
        "whisp://agent",
        serde_json::json!({ "pills": pills, "approvals": approvals }),
    );
}

fn show_island(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("island") {
        // Passive: show without stealing keyboard focus.
        let _ = w.show();
    }
}

/// One agent event, ready to display. Fire-and-forget: never blocks, never
/// logs payload content.
pub fn handle_event(app: &AppHandle, v: serde_json::Value) {
    let raw = v
        .get("hook_event_name")
        .or_else(|| v.get("event"))
        .and_then(|e| e.as_str())
        .unwrap_or("");
    let event = canonical(raw);
    let agent = ["whisp_agent", "coucou_agent", "agent"]
        .iter()
        .filter_map(|k| v.get(k).and_then(|x| x.as_str()))
        .find(|s| valid_agent(s))
        .unwrap_or("agent")
        .to_string();
    let session = str_field(&v, "session_id")
        .unwrap_or(&agent)
        .to_string();
    let cwd = str_field(&v, "cwd").map(|s| s.to_string());

    let Some(state) = app.try_state::<IslandState>() else {
        return;
    };
    let key = format!("{agent}\0{session}");
    {
        let mut pills = match state.pills.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if event == "SessionEnd" {
            pills.remove(&key);
        } else {
            let now = now_secs();
            let entry = pills.entry(key).or_insert_with(|| Pill {
                session_id: session.clone(),
                agent: agent.clone(),
                state: PillState::Idle,
                last_line: String::new(),
                cwd: None,
                pending_approval: false,
                updated_at: now,
            });
            entry.session_id = session;
            entry.agent = agent;
            match event {
                "SessionStart" => {
                    entry.state = PillState::Idle;
                    entry.pending_approval = false;
                }
                "UserPromptSubmit" => {
                    entry.state = PillState::Thinking;
                    entry.pending_approval = false;
                }
                "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "Notification"
                | "SubagentStart" | "SubagentStop" => {
                    entry.state = PillState::Working;
                }
                "PermissionRequest" => {
                    entry.state = PillState::Working;
                    entry.pending_approval = true;
                }
                "Stop" => {
                    entry.state = PillState::Finished;
                    entry.pending_approval = false;
                }
                "StopFailure" => {
                    entry.state = PillState::Error;
                    entry.pending_approval = false;
                }
                _ => {}
            }
            let line = ticker_line(event, &v);
            if !line.is_empty() {
                entry.last_line = line;
            }
            if let Some(c) = cwd {
                entry.cwd = Some(c);
            }
            entry.updated_at = now;
        }
    }
    state.touch();
    emit_full(app);
    show_island(app);
}

/// Build the permission card for a normalized `PermissionRequest` payload.
/// `tool` is the tool name; `command` is the most useful detail (command,
/// file path, pattern…). `questions` is `Some` for AskUserQuestion, whose card
/// shows the options instead of Allow/Deny. Never logs content.
pub fn approval_card(payload: &serde_json::Value, request_id: &str) -> ApprovalCard {
    let agent = ["whisp_agent", "coucou_agent", "agent"]
        .iter()
        .filter_map(|k| payload.get(k).and_then(|x| x.as_str()))
        .find(|s| valid_agent(s))
        .unwrap_or("agent")
        .to_string();
    let session = str_field(payload, "session_id")
        .unwrap_or(&agent)
        .to_string();
    let tool = str_field(payload, "tool_name").unwrap_or("tool").to_string();
    let command = ["command", "file_path", "path", "query", "pattern", "url"]
        .iter()
        .filter_map(|k| str_field(payload, k))
        .next()
        .map(short)
        .unwrap_or_default();
    let questions = asked_questions(payload);
    ApprovalCard {
        request_id: request_id.to_string(),
        session_id: session,
        agent,
        tool,
        command,
        questions,
    }
}

/// AskUserQuestion input → pickable questions, or `None` when it is not one.
fn asked_questions(payload: &serde_json::Value) -> Option<Vec<ApprovalQuestion>> {
    if payload.get("tool_name").and_then(|t| t.as_str()) != Some("AskUserQuestion") {
        return None;
    }
    let input = payload.get("tool_input")?;
    let items = input.get("questions")?.as_array()?;
    if items.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for item in items {
        let question = item.get("question")?.as_str()?.trim();
        if question.is_empty() {
            return None;
        }
        let options: Vec<String> = item
            .get("options")
            .and_then(|o| o.as_array())
            .map(|opts| {
                opts.iter()
                    .filter_map(|o| o.get("label").and_then(|l| l.as_str()))
                    .map(|s| short(s))
                    .collect()
            })
            .unwrap_or_default();
        if options.is_empty() {
            return None;
        }
        let multi = item.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false);
        out.push(ApprovalQuestion {
            question: short(question),
            options,
            multi,
        });
    }
    Some(out)
}

/// STA hook: when STA launches an agent, its pill appears in `working` state.
/// `summary` is the raw user transcript (ticker truncates it); call only with
/// non-empty text. Fire-and-forget.
pub fn upsert_sta_pill(app: &AppHandle, session_id: String, agent: String, summary: String) {
    let Some(state) = app.try_state::<IslandState>() else {
        return;
    };
    let line = short(&summary);
    {
        let mut pills = match state.pills.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let key = format!("{agent}\0{session_id}");
        let now = now_secs();
        let entry = pills.entry(key).or_insert_with(|| Pill {
            session_id: session_id.clone(),
            agent: agent.clone(),
            state: PillState::Working,
            last_line: String::new(),
            cwd: None,
            pending_approval: false,
            updated_at: now,
        });
        entry.session_id = session_id;
        entry.agent = agent;
        entry.state = PillState::Working;
        entry.pending_approval = false;
        if !line.is_empty() {
            entry.last_line = line;
        }
        entry.updated_at = now;
    }
    state.touch();
    emit_full(app);
    show_island(app);
}

/// Background reaper: hides the island after [`IDLE_HIDE`] without activity.
/// Pills are kept, so the next event restores the full list.
pub fn spawn_idle_reaper(app: AppHandle) {
    std::thread::Builder::new()
        .name("whisp-island-reap".to_string())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let idle = app
                .try_state::<IslandState>()
                .map(|s| {
                    s.last_activity
                        .lock()
                        .map(|g| g.as_ref().map(|t| t.elapsed() >= IDLE_HIDE).unwrap_or(false))
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if idle {
                if let Some(w) = app.get_webview_window("island") {
                    let _ = w.hide();
                }
            }
        })
        .ok();
}

#[tauri::command]
pub fn island_list(state: State<'_, IslandState>) -> Vec<Pill> {
    state.snapshot()
}

/// Current permission cards, oldest first. The island pulls this after the
/// `whisp://agent` snapshot event and answers via `agent_decide`.
#[tauri::command]
pub fn approval_list(state: State<'_, IslandState>) -> Vec<ApprovalCard> {
    state.approvals_snapshot()
}

#[tauri::command]
pub fn island_dismiss(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("island") {
        let _ = w.hide();
    }
    Ok(())
}

/// Mini-terminal: attach a pill to a tmux session for live view + input.
///
/// Target is derived from the pill's `session_id` (`whisp-<sanitized>`), with
/// fallback to the shared STA terminal (`whisp-sta`). Hook sessions started
/// outside tmux have no target: both commands then fail with
/// "no terminal for this session" and the UI falls back to the ticker line.
/// Never logs content; tmux calls are local and fast (<100 ms).
const TERM_LINES: usize = 40;
const TERM_MAX: usize = 16_000;
const TERM_SEND_MAX: usize = 4_000;

fn tmux_target(session_id: &str) -> Option<String> {
    let clean: String = session_id
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let clean = clean.trim_matches(|c| c == '-' || c == '_');
    if clean.is_empty() || clean.len() > 48 {
        return None;
    }
    Some(format!("whisp-{clean}"))
}

fn tmux_capture(target: &str) -> Option<String> {
    let out = std::process::Command::new("tmux")
        .args(["capture-pane", "-p", "-t", target])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(TERM_LINES);
    let mut s = lines[start..].join("\n");
    if s.len() > TERM_MAX {
        s = s[s.len() - TERM_MAX..].to_string();
    }
    Some(s)
}

#[tauri::command]
pub fn island_term_view(session_id: String) -> Result<String, String> {
    let primary = tmux_target(&session_id);
    if let Some(t) = primary.as_deref().and_then(tmux_capture) {
        return Ok(t);
    }
    // Shared STA terminal (dictation sessions all land there).
    if let Some(t) = tmux_capture("whisp-sta") {
        return Ok(t);
    }
    Err("no terminal for this session".to_string())
}

#[tauri::command]
pub fn island_term_send(session_id: String, text: String) -> Result<(), String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("empty".to_string());
    }
    if t.len() > TERM_SEND_MAX {
        return Err("too long".to_string());
    }
    let targets: Vec<String> = tmux_target(&session_id).into_iter().chain(["whisp-sta".to_string()]).collect();
    for target in &targets {
        let ok = std::process::Command::new("tmux")
            .args(["send-keys", "-t", target, "--", t, "Enter"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Ok(());
        }
    }
    Err("no terminal for this session".to_string())
}
