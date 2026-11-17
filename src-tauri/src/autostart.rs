//! Login autostart for whisp.
//!
//! - Linux: the systemd user service `whisp.service` (install.sh step 9).
//!   The Settings toggle `Lancer au démarrage` drives this:
//!   `get_autostart` is the source of truth at mount. Before this module
//!   the toggle only wrote localStorage, so the service installed with
//!   `systemctl --user enable` kept launching the app at login no matter what.
//! - macOS: a LaunchAgent plist
//!   (`~/Library/LaunchAgents/com.whisp.app.plist`) bootstrapped into the
//!   GUI domain. No systemd there.
//! - iOS / other: the mobile lifecycle owns the app, no login autostart.
//!   Both functions degrade (false / Err) so callers keep working.

use std::time::Duration;

#[cfg(target_os = "linux")]
const SERVICE: &str = "whisp.service";

#[cfg(target_os = "macos")]
const LAUNCH_LABEL: &str = "com.whisp.app";

#[cfg(target_os = "linux")]
pub fn is_enabled() -> bool {
    // `systemctl is-enabled` exits non-zero when disabled/missing/indirect —
    // all of those mean "won't launch at login". Never fails.
    match crate::run_timed_out(
        &["systemctl", "--user", "is-enabled", SERVICE],
        Duration::from_secs(5),
    ) {
        Ok(out) => out.trim() == "enabled",
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    launch_plist_path().is_some_and(|p| p.is_file())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub fn set_enabled(enabled: bool) -> Result<bool, String> {
    // Enable/disable the user service, returns the resulting state so the
    // frontend resyncs on reality (not on intent). `disable` only prevents
    // future logins from starting it; a running instance is left alone.
    let op = if enabled { "enable" } else { "disable" };
    crate::run_timed_out(
        &["systemctl", "--user", op, SERVICE],
        Duration::from_secs(10),
    )
    .map_err(|e| format!("systemctl {op} {SERVICE}: {e}"))?;
    Ok(is_enabled())
}

#[cfg(target_os = "macos")]
pub fn set_enabled(enabled: bool) -> Result<bool, String> {
    let path = launch_plist_path().ok_or_else(|| "no home directory".to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
        let plist = format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
                "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                "<plist version=\"1.0\">\n<dict>\n",
                "\t<key>Label</key>\n\t<string>{label}</string>\n",
                "\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{exe}</string>\n\t</array>\n",
                "\t<key>RunAtLoad</key>\n\t<true/>\n",
                "</dict>\n</plist>\n"
            ),
            label = LAUNCH_LABEL,
            exe = exe.display()
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir launchagents: {e}"))?;
        }
        std::fs::write(&path, plist).map_err(|e| format!("write plist: {e}"))?;
        // Bootstrap into the GUI domain; an already-bootstrapped label is
        // fine (idempotent toggle), so errors here never fail the toggle —
        // the plist on disk is the source of truth, like systemctl enable.
        let uid = current_uid();
        let _ = crate::run_timed_out(
            &[
                "launchctl",
                "bootstrap",
                &format!("gui/{uid}"),
                &path.to_string_lossy(),
            ],
            Duration::from_secs(10),
        );
    } else {
        let uid = current_uid();
        let _ = crate::run_timed_out(
            &["launchctl", "bootout", &format!("gui/{uid}/{LAUNCH_LABEL}")],
            Duration::from_secs(10),
        );
        // Missing file = already off.
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("remove plist: {e}"))?;
        }
    }
    Ok(is_enabled())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn set_enabled(_enabled: bool) -> Result<bool, String> {
    Err("login autostart is not supported on this platform".to_string())
}

#[cfg(target_os = "macos")]
fn launch_plist_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library/LaunchAgents")
            .join(format!("{LAUNCH_LABEL}.plist"))
    })
}

#[cfg(target_os = "macos")]
fn current_uid() -> u32 {
    // SAFETY: getuid is always-success, no side effects.
    unsafe { libc::getuid() }
}
