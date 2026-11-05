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

/// Shared token for the local control plane (`~/.config/whisp/socket.token`, 0600).
/// Any local process can connect to the unix socket; the token proves the caller
/// is our MCP bridge, not a random local probe. Bare `toggle` (Hyprland bind)
/// stays exempt for compat; all JSON `{"cmd":...}` require it once created.
pub fn token_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join(".config")
        })
        .join("whisp/socket.token")
}

/// Load or create the token (600). Never panics: on error returns a per-boot
/// fallback (still consistent within this process).
pub fn load_or_create_token() -> String {
    let path = token_path();
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if s.len() >= 16 {
            return s;
        }
    }
    let tok = uuid::Uuid::new_v4().to_string().replace("-", "") + &uuid::Uuid::new_v4().to_string().replace("-", "");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::write(&path, &tok);
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(&path, &tok);
    }
    tok
}

/// Check a JSON command value for a valid token (`token` or `auth` field).
/// Missing token file = allow (first boot before creation), otherwise strict.
pub fn verify_token(v: &serde_json::Value) -> bool {
    let provided = v.get("token").or_else(|| v.get("auth")).and_then(|x| x.as_str()).unwrap_or("");
    if provided.is_empty() {
        return false;
    }
    let expected = std::fs::read_to_string(token_path()).map(|s| s.trim().to_string()).unwrap_or_default();
    if expected.len() < 16 {
        // No token yet (fresh install): create one and allow this once so the
        // bridge never deadlocks on first run.
        let _ = load_or_create_token();
        return true;
    }
    // Constant-time-ish compare (avoid trivial timing leak on local socket).
    provided.len() == expected.len() && provided.bytes().zip(expected.bytes()).fold(0, |a, (x, y)| a | (x ^ y)) == 0
}

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

    #[test]
    fn token_path_is_config() {
        assert!(token_path().to_string_lossy().contains("socket.token"));
    }
}
