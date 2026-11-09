//! Autostart via the systemd user service `whisp.service` (install.sh step 9).
//!
//! The Settings toggle `Lancer au démarrage` drives this: `get_autostart`
//! is the source of truth at mount. Before this module the toggle only wrote
//! localStorage, so the service installed with `systemctl --user enable`
//! kept launching the app at login no matter what.
//!
use std::time::Duration;

const SERVICE: &str = "whisp.service";

/// `systemctl is-enabled` exits non-zero when disabled/missing/indirect —
/// all of those mean "won't launch at login". Never fails.
pub fn is_enabled() -> bool {
    match crate::run_timed_out(
        &["systemctl", "--user", "is-enabled", SERVICE],
        Duration::from_secs(5),
    ) {
        Ok(out) => out.trim() == "enabled",
        Err(_) => false,
    }
}

/// Enable/disable the user service, returns the resulting state so the
/// frontend resyncs on reality (not on intent). `disable` only prevents
/// future logins from starting it; a running instance is left alone.
pub fn set_enabled(enabled: bool) -> Result<bool, String> {
    let op = if enabled { "enable" } else { "disable" };
    crate::run_timed_out(
        &["systemctl", "--user", op, SERVICE],
        Duration::from_secs(10),
    )
    .map_err(|e| format!("systemctl {op} {SERVICE}: {e}"))?;
    Ok(is_enabled())
}
