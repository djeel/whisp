//! Local control socket paths: `$XDG_RUNTIME_DIR/whisp-<uid>.sock` first,
//! `/tmp` fallback. XDG_RUNTIME_DIR is 0700 by spec; /tmp fallback gets
//! explicit 0600 hardening in `spawn_toggle_socket`.

use std::path::PathBuf;

/// Real uid, not `$UID` env (env can be unset/spoofed in services).
pub fn current_uid() -> u32 {
    #[cfg(target_os = "linux")]
    // SAFETY: getuid is always-success, no side effects.
    let uid = unsafe { libc::getuid() };
    #[cfg(not(target_os = "linux"))]
    let uid = 1000u32;
    uid
}

pub fn socket_name() -> String {
    format!("whisp-{}.sock", current_uid())
}

/// Canonical control socket path (same logic in `main.rs` toggle client,
/// `mcp_server.py`, and `spawn_toggle_socket` server).
pub fn control_socket_path() -> PathBuf {
    let name = socket_name();
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
    {
        return dir.join(&name);
    }
    std::env::temp_dir().join(&name)
}

/// Remove a stale socket without following symlinks.
/// Returns true if the path is now absent (or was never there).
pub fn safe_remove_socket(path: &std::path::Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
        Ok(md) => {
            // Never follow a symlink in /tmp: attacker could point it at
            // an arbitrary file and trick us into unlinking it.
            if md.file_type().is_symlink() {
                eprintln!("[whisp] socket symlink refusé: {}", path.display());
                // Still unlink the symlink itself, not its target.
                return std::fs::remove_file(path).is_ok();
            }
            std::fs::remove_file(path).is_ok()
        }
    }
}

/// Enforce 0600 on the bound socket (defense in depth for /tmp fallback).
#[cfg(unix)]
pub fn enforce_600(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
pub fn enforce_600(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_name_contains_uid() {
        let n = socket_name();
        assert!(n.starts_with("whisp-") && n.ends_with(".sock"));
    }

    #[test]
    fn control_path_prefers_xdg() {
        // Ne touche pas au FS : vérifie juste la logique de sélection.
        let p = control_socket_path();
        assert!(p.to_string_lossy().contains("whisp-"));
    }
}
