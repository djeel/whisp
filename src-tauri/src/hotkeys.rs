//! Global hotkeys for whisp (Tauri `global-shortcut` plugin).
//!
//! Defaults:
//! - Toggle recording: `Alt+Space`
//! - Push-to-talk (hold): `Ctrl+Alt+Space` (Pressed = start, Released = stop)
//! - Cycle mode: `Ctrl+Alt+M`
//! - Cancel: `Esc`, handled window-locally by the recorder frontend
//!   (grabbing Esc globally would steal it from every other app).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleRecording,
    PushToTalkPressed,
    PushToTalkReleased,
    CycleMode,
}

pub fn toggle_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    Shortcut::new(Some(Modifiers::ALT), Code::Space)
}

pub fn push_to_talk_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::Space,
    )
}

pub fn cycle_mode_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::KeyM,
    )
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
