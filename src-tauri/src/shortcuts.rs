//! Shortcuts settings backend: live inventory + Hyprland reload.
//!
//! Global shortcuts live in config (`hotkey_toggle`…), parsed by
//! `hotkeys.rs` (`Alt+Space` toggle, `Ctrl+Alt+Space` push-to-talk,
//! `Ctrl+Alt+M` cycle, Esc cancel handled in the recorder window).
//! This module exposes them (pretty labels) to the settings UI and
//! reloads the Hyprland config (`hyprctl reload`) after the user edits
//! their binds externally.

#[derive(Debug, Clone, serde::Serialize)]
pub struct ShortcutInfo {
    pub id: String,
    pub label: String,
    pub keys: String,
    pub description: String,
}

/// All whisp shortcuts, reading live config + recorder Esc handling.
/// `editable=false` = fixe (cancel local) ; les 3 autres sont configurables.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShortcutRow {
    pub id: String,
    pub label: String,
    pub keys: String,
    pub raw: String,
    pub editable: bool,
    pub description: String,
}

/// Live inventory for the settings UI (pretty French labels).
pub fn all_shortcut_rows() -> Vec<ShortcutRow> {
    let (toggle, ptt, cycle) = crate::hotkeys::current();
    vec![
        ShortcutRow {
            id: "toggle".to_string(),
            label: "Dicter / arrêter".to_string(),
            raw: toggle.clone(),
            keys: crate::hotkeys::pretty(&toggle),
            editable: true,
            description: "Bascule l'enregistrement (global)".to_string(),
        },
        ShortcutRow {
            id: "push-to-talk".to_string(),
            label: "Push-to-talk".to_string(),
            raw: ptt.clone(),
            keys: format!("{} (maintenir)", crate::hotkeys::pretty(&ptt)),
            editable: true,
            description: "Enregistre tant que les touches sont maintenues".to_string(),
        },
        ShortcutRow {
            id: "cycle-mode".to_string(),
            label: "Changer de mode".to_string(),
            raw: cycle.clone(),
            keys: crate::hotkeys::pretty(&cycle),
            editable: true,
            description: "Cycle Message → Mail → Voice → Agent".to_string(),
        },
        ShortcutRow {
            id: "cancel".to_string(),
            label: "Annuler".to_string(),
            raw: "Escape".to_string(),
            keys: "Échap".to_string(),
            editable: false,
            description: "Annule la dictée en cours (fenêtre popup)".to_string(),
        },
    ]
}

/// Legacy inventory (raw `ShortcutInfo`) for the socket MCP contract.
pub fn all_shortcuts() -> Vec<ShortcutInfo> {
    all_shortcut_rows()
        .into_iter()
        .map(|r| ShortcutInfo {
            id: r.id,
            label: r.label,
            keys: r.keys,
            description: r.description,
        })
        .collect()
}

/// Reload the Hyprland config so edited binds apply now.
/// `hyprctl reload` exit code is surfaced; missing hyprctl = clear error.
 fn reload_hyprland_inner() -> Result<String, String> {
    // Deadline 15 s : hyprctl coincé (service sans WAYLAND_DISPLAY) = erreur.
    match crate::run_timed_out(&["hyprctl", "reload"], std::time::Duration::from_secs(15)) {
        Ok(_) => Ok("Configuration Hyprland rechargée.".to_string()),
        Err(e) if e.contains("No such file") || e.contains("spawn hyprctl") => {
            Err("hyprctl introuvable (Hyprland requis)".to_string())
        }
        Err(e) => Err(format!("hyprctl reload: {e}")),
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
 
/// Live inventory with raw strings + editable flags (settings UI).
#[tauri::command]
pub fn list_hotkeys() -> Vec<ShortcutRow> {
    all_shortcut_rows()
}

/// Change one global shortcut (`toggle` | `push-to-talk` | `cycle-mode`).
/// Valide le format, persiste, désenregistre l'ancien + enregistre le
/// nouveau sans redémarrage. Erreur = rien n'a changé (ancien toujours actif).
#[tauri::command]
pub fn set_hotkey(app: tauri::AppHandle, id: String, keys: String) -> Result<String, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let raw = keys.trim().to_string();
    if raw.is_empty() {
        return Err("raccourci vide — ex. alt+Space".to_string());
    }
    // 1. Parse d'abord : un format invalide ne touche à rien.
    let new_sc = crate::hotkeys::parse_shortcut(&raw)?;
    // 2. Ancien shortcut (pour le désenregistrer).
    let old_sc = match id.as_str() {
        "toggle" => crate::hotkeys::toggle_shortcut(),
        "push-to-talk" => crate::hotkeys::push_to_talk_shortcut(),
        "cycle-mode" => crate::hotkeys::cycle_mode_shortcut(),
        _ => return Err(format!("unknown hotkey id '{id}' (toggle|push-to-talk|cycle-mode)")),
    };
    // 3. Désenregistre l'ancien, enregistre le nouveau, persiste si OK.
    // En cas d'échec (conflit), restaure l'ancien pour ne jamais perdre
    // tous les raccourcis.
    app.global_shortcut()
        .unregister(old_sc)
        .map_err(|e| format!("désenregistrement {old_sc} : {e}"))?;
    if let Err(e) = app.global_shortcut().register(new_sc) {
        let _ = app.global_shortcut().register(old_sc);
        return Err(format!("{new_sc} refusé ({e}) — ancien conservé"));
    }
    // 4. Persiste via config::set (merge atomique, jamais de perte).
    let (nt, np, nc) = match id.as_str() {
        "toggle" => (Some(raw.clone()), None, None),
        "push-to-talk" => (None, Some(raw.clone()), None),
        _ => (None, None, Some(raw.clone())),
    };
    crate::config::set(
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None, None, None, None, nt, np, nc,
    )?;
    Ok(format!("{} → {}", crate::hotkeys::pretty(&raw), raw))
}

 /// Run `hyprctl reload` so edited binds apply immediately.
 #[tauri::command]
 pub fn reload_hyprland() -> Result<String, String> {
     reload_hyprland_inner()
 }
