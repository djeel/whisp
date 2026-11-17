//! OS clipboard + keystroke injection for paste / undo-paste.
//!
//! - Linux: `wl-copy`/`wl-paste` + `wtype` (Wayland) with `ydotool`/`xdotool`
//!   fallbacks — the existing pipeline, unchanged.
//! - macOS: `pbcopy`/`pbpaste` + `osascript` keystroke `Cmd+V` / `BackSpace`
//!   (no new dependency; macOS Accessibility permission required, same as
//!   any injector).
//! - iOS / other: unsupported (no global paste); every entry point returns
//!   "not done" so callers degrade to clipboard-only or history-only.

use std::time::Duration;

/// Max wait for one paste/undo injection.
const PASTE_TIMEOUT: Duration = Duration::from_secs(5);

/// True on desktop OSes where global paste injection exists.
pub fn paste_supported() -> bool {
    cfg!(target_os = "linux") || cfg!(target_os = "macos")
}

/// Copy `text` to the system clipboard. False when empty or on failure.
pub fn copy_to_clipboard(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    #[cfg(target_os = "linux")]
    return run_stdin("wl-copy", &["--type", "text/plain"], text);
    #[cfg(target_os = "macos")]
    return run_stdin("pbcopy", &[], text);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return false;
}

/// Previous clipboard content (captured BEFORE overwrite, restored by undo).
pub fn read_clipboard_before() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        match run_timed_out(&["wl-paste", "--type", "text/plain", "--no-newline"], PASTE_TIMEOUT)
        {
            Ok(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    }
    #[cfg(target_os = "macos")]
    {
        match run_timed_out(&["pbpaste"], PASTE_TIMEOUT) {
            Ok(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Send the paste keystroke (Ctrl+V on Linux, Cmd+V on macOS) to the
/// focused app. The text itself travels through the clipboard (unicode-safe).
pub fn paste_keystroke() -> bool {
    #[cfg(target_os = "linux")]
    {
        run_quiet("wtype", &["-M", "ctrl", "-k", "v", "-m", "ctrl"])
            || run_quiet("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"])
            || run_quiet("xdotool", &["key", "ctrl+v"])
    }
    #[cfg(target_os = "macos")]
    {
        // `keystroke "v" using command down` follows the user keyboard
        // layout (V stays the paste key on AZERTY and QWERTY alike).
        run_quiet(
            "osascript",
            &[
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using command down",
            ],
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// Send `n` BackSpace keys (capped) to erase the last paste.
pub fn press_backspace(n: usize) -> bool {
    let n = n.min(5000);
    if n == 0 {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        // wtype accepts a `-k` sequence in a single call.
        let mut wtype_argv: Vec<String> = Vec::with_capacity(n * 2);
        for _ in 0..n {
            wtype_argv.push("-k".to_string());
            wtype_argv.push("BackSpace".to_string());
        }
        let mut wtype_refs: Vec<&str> = vec!["wtype"];
        wtype_refs.extend(wtype_argv.iter().map(|s| s.as_str()));
        if run_timed(&wtype_refs, PASTE_TIMEOUT).is_ok() {
            return true;
        }
        let mut y_argv: Vec<String> = Vec::with_capacity(n * 2);
        for _ in 0..n {
            y_argv.push("14:1".to_string());
            y_argv.push("14:0".to_string());
        }
        let mut y_refs: Vec<&str> = vec!["ydotool", "key"];
        y_refs.extend(y_argv.iter().map(|s| s.as_str()));
        if run_timed(&y_refs, PASTE_TIMEOUT).is_ok() {
            return true;
        }
        let mut x_argv: Vec<&str> = Vec::with_capacity(n + 2);
        x_argv.push("xdotool");
        x_argv.push("key");
        for _ in 0..n {
            x_argv.push("BackSpace");
        }
        run_timed(&x_argv, PASTE_TIMEOUT).is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to repeat {n} times\nkey code 51\nend repeat"
        );
        run_timed(&["osascript", "-e", &script], PASTE_TIMEOUT).is_ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Local run helpers (same contract as the lib.rs ones, kept here so this
// module stays self-contained and unit-testable without Tauri).
// ---------------------------------------------------------------------------

fn run_quiet(bin: &str, args: &[&str]) -> bool {
    std::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_stdin(bin: &str, args: &[&str], input: &str) -> bool {
    std::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(input.as_bytes())?;
            }
            child.wait()?;
            Ok::<_, std::io::Error>(())
        })
        .map(|_| true)
        .unwrap_or(false)
}

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

fn run_timed_out(argv: &[&str], timeout: Duration) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_copy_is_rejected() {
        assert!(!copy_to_clipboard("   "));
    }

    #[test]
    fn backspace_zero_is_noop_ok() {
        assert!(press_backspace(0));
    }

    #[test]
    fn paste_supported_matches_desktop() {
        assert_eq!(
            paste_supported(),
            cfg!(target_os = "linux") || cfg!(target_os = "macos")
        );
    }
}
