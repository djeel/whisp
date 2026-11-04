//! Shortcuts settings backend: static shortcut inventory + Hyprland reload.
//!
//! The global shortcuts are fixed in `hotkeys.rs` (Alt+Space toggle,
//! Ctrl+Alt+Space push-to-talk, Ctrl+Alt+M cycle, Esc cancel handled in the
//! recorder window). This module exposes them to the settings UI and
//! reloads the Hyprland config (`hyprctl reload`) after the user edits
//! their binds externally.

#[derive(Debug, Clone, serde::Serialize)]
pub struct ShortcutInfo {
    pub id: String,
    pub label: String,
    pub keys: String,
    pub description: String,
}

/// All whisp shortcuts, matching `hotkeys.rs` + recorder Esc handling.
pub fn all_shortcuts() -> Vec<ShortcutInfo> {
    vec![
        ShortcutInfo {
            id: "toggle".to_string(),
            label: "Dicter / arrêter".to_string(),
            keys: "Alt + Espace".to_string(),
            description: "Bascule l'enregistrement (global)".to_string(),
        },
        ShortcutInfo {
            id: "push-to-talk".to_string(),
            label: "Push-to-talk".to_string(),
            keys: "Ctrl + Alt + Espace (maintenir)".to_string(),
            description: "Enregistre tant que les touches sont maintenues".to_string(),
        },
        ShortcutInfo {
            id: "cycle-mode".to_string(),
            label: "Changer de mode".to_string(),
            keys: "Ctrl + Alt + M".to_string(),
            description: "Cycle Message → Super → Mail → Note → Voice → Meeting".to_string(),
        },
        ShortcutInfo {
            id: "cancel".to_string(),
            label: "Annuler".to_string(),
            keys: "Échap".to_string(),
            description: "Annule la dictée en cours (fenêtre popup)".to_string(),
        },
    ]
}

/// Reload the Hyprland config so edited binds apply now.
/// `hyprctl reload` exit code is surfaced; missing hyprctl = clear error.
fn reload_hyprland_inner() -> Result<String, String> {
    let out = std::process::Command::new("hyprctl")
        .arg("reload")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "hyprctl introuvable (Hyprland requis)".to_string()
            } else {
                format!("hyprctl: {e}")
            }
        })?;
    if out.status.success() {
        Ok("Configuration Hyprland rechargée.".to_string())
    } else {
        Err(format!(
            "hyprctl reload: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// List all whisp shortcuts for the settings UI.
#[tauri::command]
pub fn list_shortcuts() -> Vec<ShortcutInfo> {
    all_shortcuts()
}

/// Run `hyprctl reload` so edited binds apply immediately.
#[tauri::command]
pub fn reload_hyprland() -> Result<String, String> {
    reload_hyprland_inner()
}
