//! Global hotkeys for whisp (Tauri `global-shortcut` plugin).
//!
//! Defaults (overridable in Settings → Raccourcis, persisted in config):
//! - Toggle recording: `alt+Space`
//! - Push-to-talk (hold): `control+alt+Space` (Pressed = start, Released = stop)
//! - Cycle mode: `control+alt+M`
//! - Cancel: `Esc`, handled window-locally by the recorder frontend
//!   (grabbing Esc globally would steal it from every other app).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleRecording,
    PushToTalkPressed,
    PushToTalkReleased,
    CycleMode,
}

pub const DEFAULT_TOGGLE: &str = "alt+Space";
pub const DEFAULT_PUSH_TO_TALK: &str = "control+alt+Space";
pub const DEFAULT_CYCLE_MODE: &str = "control+alt+M";

/// Parse `alt+Space` / `control+shift+X` into a Tauri Shortcut.
/// Accepts `ctrl`/`control`, `alt`/`option`, `shift`, `super`/`meta`/`cmd`,
/// case-insensitive, ordered modifiers-then-key (global-hotkey format).
pub fn parse_shortcut(raw: &str) -> Result<tauri_plugin_global_shortcut::Shortcut, String> {
    raw.parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("raccourci invalide '{raw}' (ex. alt+Space) : {e}"))
}

/// Configured shortcut or compiled default. Never panics: invalid stored
/// values fall back to the default (logged once).
fn configured_or_default(raw: Option<String>, def: &str) -> tauri_plugin_global_shortcut::Shortcut {
    if let Some(s) = raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match parse_shortcut(s) {
            Ok(sc) => return sc,
            Err(e) => eprintln!("[whisp] hotkey ignoré ({e}), défaut {def}"),
        }
    }
    parse_shortcut(def).expect("default shortcut must parse")
}

pub fn toggle_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    configured_or_default(crate::config::load().hotkey_toggle, DEFAULT_TOGGLE)
}

pub fn push_to_talk_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    configured_or_default(crate::config::load().hotkey_push_to_talk, DEFAULT_PUSH_TO_TALK)
}

pub fn cycle_mode_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    configured_or_default(crate::config::load().hotkey_cycle_mode, DEFAULT_CYCLE_MODE)
}

/// Map a triggered shortcut + press state to an action.
pub fn match_action(
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
    pressed: bool,
) -> Option<HotkeyAction> {
    if *shortcut == toggle_shortcut() {
        return if pressed {
            Some(HotkeyAction::ToggleRecording)
        } else {
            None
        };
    }
    if *shortcut == push_to_talk_shortcut() {
        return Some(if pressed {
            HotkeyAction::PushToTalkPressed
        } else {
            HotkeyAction::PushToTalkReleased
        });
    }
    if *shortcut == cycle_mode_shortcut() {
        return if pressed {
            Some(HotkeyAction::CycleMode)
        } else {
            None
        };
    }
    None
}

/// Register the global shortcuts. Individual failures are collected and
/// returned so the app can still start (e.g. conflicting keybinds).
pub fn register_all(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let mut errors = Vec::new();
    for sc in [
        toggle_shortcut(),
        push_to_talk_shortcut(),
        cycle_mode_shortcut(),
    ] {
        if let Err(e) = app.global_shortcut().register(sc) {
            errors.push(format!("{sc}: {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Unregister everything (before re-registering after a user change).
pub fn unregister_all(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("unregister: {e}"))
}

/// Human label for the settings UI (`alt+Space` → `Alt + Espace`).
pub fn pretty(raw: &str) -> String {
    raw.split('+')
        .map(|t| {
            let l = t.trim().to_lowercase();
            match l.as_str() {
                "control" | "ctrl" => "Ctrl".to_string(),
                "alt" | "option" => "Alt".to_string(),
                "shift" => "Maj".to_string(),
                "super" | "meta" | "cmd" => "Super".to_string(),
                "space" => "Espace".to_string(),
                _ => t.trim().to_uppercase(),
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}
/// Current shortcuts as stored (raw strings, defaults when unset).
pub fn current() -> (String, String, String) {
    let cfg = crate::config::load();
    (
        cfg.hotkey_toggle
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_TOGGLE.to_string()),
        cfg.hotkey_push_to_talk
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PUSH_TO_TALK.to_string()),
        cfg.hotkey_cycle_mode
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CYCLE_MODE.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_canonical_forms() {
        assert_eq!(parse_shortcut("alt+Space").unwrap().to_string(), "alt+Space");
        assert_eq!(parse_shortcut("control+alt+Space").unwrap().to_string(), "control+alt+Space");
        assert_eq!(parse_shortcut("ctrl+shift+F1").unwrap().to_string(), "shift+control+F1");
        assert!(parse_shortcut("").is_err());
        assert!(parse_shortcut("control+alt").is_err());
    }

    #[test]
    fn pretty_french_labels() {
        assert_eq!(pretty("alt+Space"), "Alt + Espace");
        assert_eq!(pretty("control+alt+Space"), "Ctrl + Alt + Espace");
        assert_eq!(pretty("control+alt+M"), "Ctrl + Alt + M");
    }
}
