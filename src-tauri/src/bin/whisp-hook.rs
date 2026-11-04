//! whisp-hook — relay stdin hook JSON to the whisp app over the agents socket.
//!
//! Usage: `... | whisp-hook [--agent <name>] [<Event>]`
//! Fire-and-forget (2 s) except `PermissionRequest`, which waits up to 110 s.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const FIRE_AND_FORGET_BUDGET: Duration = Duration::from_secs(2);
const DECISION_BUDGET: Duration = Duration::from_secs(110);

fn socket_path() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return dir.join("whisp-agents.sock");
    }
    let uid = std::env::var("UID").ok().and_then(|u| u.parse::<u32>().ok()).unwrap_or(1000);
    std::path::PathBuf::from(format!("/tmp/whisp-agents-{uid}.sock"))
}

fn valid_agent(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 24
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Agents whose permission requests the island can answer: Claude Code
/// (no `--agent`), Codex, Copilot CLI, Muse Code and Claude Desktop. Any other
/// agent's request is answered at once by us, so it asks in its own terminal.
/// MUST match `takes_decisions` in coucou's hook/src/reply.rs.
pub fn takes_decisions(agent: &str) -> bool {
    matches!(agent, "" | "codex" | "copilot" | "muse" | "claude-desktop")
}

/// Agents that read a JSON object on stdout after every hook and get `{}` —
/// "no opinion" — when there is nothing to say.
fn wants_json(agent: &str) -> bool {
    matches!(agent, "gemini" | "antigravity" | "copilot" | "muse")
}

/// The line to print for `event` from `agent`, given the island's `decision`
/// (`None`: nobody clicked) and, for AskUserQuestion, the question as asked.
/// NEVER prints anything that allows without a human decision.
pub fn stdout_reply(
    agent: &str,
    event: &str,
    decision: Option<&str>,
    question: Option<&serde_json::Value>,
) -> Option<String> {
    if event != "PermissionRequest" {
        if agent.eq_ignore_ascii_case("antigravity") && event == "PreToolUse" {
            return Some(r#"{"decision":"ask"}"#.to_string());
        }
        return wants_json(agent).then(|| "{}".to_string());
    }
    let decision = decision.filter(|_| takes_decisions(agent));
    match agent {
        "copilot" | "muse" => {
            let word = match decision.map(str::trim) {
                Some("allow" | "always") => Some("allow"),
                Some("deny") => Some("deny"),
                _ => None,
            };
            match word {
                Some(word) => Some(
                    serde_json::json!({ "permissionDecision": word }).to_string(),
                ),
                None if agent == "copilot" => {
                    Some(serde_json::json!({ "permissionDecision": "ask" }).to_string())
                }
                None => None,
            }
        }
        "" | "claude-desktop" => decision.and_then(|d| decision_json(d, question)),
        _ => decision.and_then(|d| decision_json(d, None)),
    }
}

/// Documented PermissionRequest output. Anything unrecognised prints nothing —
/// silence is the safe answer. See https://code.claude.com/docs/en/hooks
pub fn decision_json(decision: &str, question: Option<&serde_json::Value>) -> Option<String> {
    if decision.trim_start().starts_with('{') {
        let reply = serde_json::from_str::<serde_json::Value>(decision).ok()?;
        let answers = reply.get("answers")?.as_object()?;
        let question = question?;
        if !answers_fit(question, answers) {
            return None;
        }
        let mut input = question.as_object()?.clone();
        input.insert("answers".into(), serde_json::Value::Object(answers.clone()));
        return Some(
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": { "behavior": "allow", "updatedInput": input },
                }
            })
            .to_string(),
        );
    }
    let behavior = match decision.trim() {
        "allow" | "always" => r#"{"behavior":"allow"}"#.to_string(),
        "deny" => r#"{"behavior":"deny","message":"Denied from whisp"}"#.to_string(),
        _ => return None,
    };
    Some(format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{behavior}}}}}"#
    ))
}

/// True when `answers` answers exactly the questions asked: one entry per
/// question keyed by its text; single-select = one label string, multi-select
/// = non-empty list of distinct labels.
fn answers_fit(
    question: &serde_json::Value,
    answers: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    let Some(items) = question.get("questions").and_then(|q| q.as_array()) else {
        return false;
    };
    if items.is_empty() || items.len() != answers.len() {
        return false;
    }
    items.iter().all(|item| {
        let Some(text) = item.get("question").and_then(|q| q.as_str()) else {
            return false;
        };
        let labels: Vec<&str> = item
            .get("options")
            .and_then(|o| o.as_array())
            .map(|opts| {
                opts.iter()
                    .filter_map(|o| o.get("label").and_then(|l| l.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let multi = item.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false);
        match answers.get(text) {
            Some(serde_json::Value::String(pick)) if !multi => labels.contains(&pick.as_str()),
            Some(serde_json::Value::Array(picks)) if multi => {
                let picks: Vec<&str> = picks.iter().filter_map(|p| p.as_str()).collect();
                let mut seen = picks.clone();
                seen.sort_unstable();
                seen.dedup();
                !picks.is_empty() && seen.len() == picks.len() && picks.iter().all(|p| labels.contains(p))
            }
            _ => false,
        }
    })
}


fn main() {
    let deadline = Instant::now() + FIRE_AND_FORGET_BUDGET;
    let mut agent_arg = String::new();
    let mut event_arg = String::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--agent" {
            agent_arg = it.next().unwrap_or_default();
        } else if event_arg.is_empty() {
            event_arg = a;
        }
    }

    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    let mut payload = match serde_json::from_slice::<serde_json::Value>(raw) {
        Ok(v) => v,
        Err(_) => std::process::exit(0),
    };
    let Some(map) = payload.as_object_mut() else {
        std::process::exit(0);
    };
    if valid_agent(&agent_arg) {
        map.insert("coucou_agent".into(), serde_json::Value::String(agent_arg.clone()));
    }
    for f in ["tool_response", "tool_output", "transcript_path"] {
        map.remove(f);
    }
    let raw_event = map
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| event_arg.clone());
    if !raw_event.is_empty() {
        map.insert(
            "hook_event_name".into(),
            serde_json::Value::String(raw_event.clone()),
        );
    }
    let name = map
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    if map.get("cwd").and_then(serde_json::Value::as_str).map(str::is_empty).unwrap_or(true) {
        if let Ok(cwd) = std::env::current_dir() {
            map.insert("cwd".into(), serde_json::Value::String(cwd.to_string_lossy().to_string()));
        }
    }
    // Only an agent whose decisions the island can give waits for one; any
    // other agent's request would wait for a decision that is thrown away.
    let waits = name == "PermissionRequest" && takes_decisions(&agent_arg);
    // AskUserQuestion input travels back with the decision; keep a copy.
    let question = (name == "PermissionRequest"
        && payload.get("tool_name").and_then(|t| t.as_str()) == Some("AskUserQuestion"))
    .then(|| payload.get("tool_input").cloned())
    .flatten();
    let mut line = payload.to_string();
    line.push('\n');
    let budget = if waits { DECISION_BUDGET } else { deadline.saturating_duration_since(Instant::now()) };
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        let _ = tx.send(talk(&line, waits));
    });
    let decision = rx.recv_timeout(budget).ok().flatten();
    if let Some(out) = stdout_reply(&agent_arg, &name, decision.as_deref(), question.as_ref()) {
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{out}");
        let _ = stdout.flush();
    }
    // Exit 0 always: never block the calling agent.
}

fn talk(payload: &str, waits_for_answer: bool) -> Option<String> {
    let path = socket_path();
    let start = Instant::now();
    // Retry only while the backlog is full (EAGAIN), within the budget.
    let mut stream = loop {
        match try_connect(&path) {
            Ok(s) => break Some(s),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && start.elapsed() < CONNECT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => break None,
        }
    }?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(payload.as_bytes()).is_err() {
        return None;
    }
    let _ = stream.flush();
    if !waits_for_answer {
        return None;
    }
    let _ = stream.set_read_timeout(Some(DECISION_BUDGET));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    use std::io::Read;
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.contains(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let answer = String::from_utf8_lossy(&buf).trim().to_string();
    (!answer.is_empty()).then_some(answer)
}

fn try_connect(path: &std::path::Path) -> std::io::Result<std::os::unix::net::UnixStream> {
    use std::os::unix::net::UnixStream;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
    if bytes.len() >= addr.sun_path.len() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "path too long"));
    }
    for (i, &b) in bytes.iter().enumerate() {
        addr.sun_path[i] = b as libc::c_char;
    }
    let len = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    let r = unsafe {
        libc::connect(
            stream.as_raw_fd(),
            (&addr as *const libc::sockaddr_un).cast::<libc::sockaddr>(),
            len,
        )
    };
    if r == 0 {
        unsafe {
            let flags = libc::fcntl(stream.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(stream.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
        return Ok(stream);
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EINPROGRESS) {
        let mut pfd = libc::pollfd { fd: stream.as_raw_fd(), events: libc::POLLOUT, revents: 0 };
        let r = unsafe { libc::poll(&mut pfd, 1, CONNECT_TIMEOUT.as_millis() as libc::c_int) };
        if r > 0 {
            let mut so_err: libc::c_int = 0;
            let mut optlen = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut so_err as *mut libc::c_int).cast::<libc::c_void>(),
                    &mut optlen,
                );
            }
            if so_err == 0 {
                unsafe {
                    let flags = libc::fcntl(stream.as_raw_fd(), libc::F_GETFL);
                    libc::fcntl(stream.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK);
                }
                return Ok(stream);
            }
            return Err(std::io::Error::from_raw_os_error(so_err));
        }
        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "connect timeout"));
    }
    if err.raw_os_error() == Some(libc::EAGAIN) {
        return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "backlog full"));
    }
    Err(err)
}
