//! App config for whisp (`~/.config/whisp/config.json`).
//!
//! Small JSON file shared between backend and frontend: local STT model
//! (`small` by default, `medium`/`base` opt-in), recorder popup position and
//! theme. Unknown keys are ignored on read; writes merge into the loaded
//! file so partial `set_config` calls never drop other fields.

use serde::{Deserialize, Serialize};

/// Frontend contract: camelCase keys (`popupPos`), all fields optional so
/// the UI can sync one setting at a time and fall back to localStorage when
/// a key is absent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_pos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sta_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sta_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sta_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sta_context: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realtime_preview: Option<bool>,
}

fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join(".config"))
        .join("whisp/config.json")
}

pub fn load() -> AppConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("config dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| format!("config encode: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("config write: {e}"))
}

/// Merge a partial update into the stored file. `stt_model` is validated
/// (`small`/`medium`/`base`); other fields are opaque UI preferences.
pub fn set(
    popup_pos: Option<String>,
    stt_model: Option<String>,
    theme: Option<String>,
    language: Option<String>,
    input_device: Option<String>,
    sta_agent: Option<String>,
    sta_command: Option<String>,
    sta_cwd: Option<String>,
    sta_context: Option<bool>,
    realtime_preview: Option<bool>,
) -> Result<(), String> {
    if let Some(m) = &stt_model {
        if !matches!(m.as_str(), "small" | "medium" | "base") {
            return Err(format!("unknown stt_model '{m}' (small|medium|base)"));
        }
    }
    let mut cfg = load();
    if popup_pos.is_some() {
        cfg.popup_pos = popup_pos;
    }
    if stt_model.is_some() {
        cfg.stt_model = stt_model;
    }
    if theme.is_some() {
        cfg.theme = theme;
    }
    if let Some(l) = &language {
        if !matches!(l.as_str(), "fr" | "en" | "auto") {
            return Err(format!("unknown language '{l}' (fr|en|auto)"));
        }
    }
    if language.is_some() {
        cfg.language = language;
    }
    if input_device.is_some() {
        cfg.input_device = input_device;
    }
    if let Some(a) = &sta_agent {
        if !matches!(a.as_str(), "omp" | "claude" | "codex" | "custom") {
            return Err(format!("unknown sta_agent '{a}' (omp|claude|codex|custom)"));
        }
    }
    if sta_agent.is_some() {
        cfg.sta_agent = sta_agent;
    }
    if sta_command.is_some() {
        cfg.sta_command = sta_command;
    }
    if sta_cwd.is_some() {
        cfg.sta_cwd = sta_cwd;
    }
    if sta_context.is_some() {
        cfg.sta_context = sta_context;
    }
    if realtime_preview.is_some() {
        cfg.realtime_preview = realtime_preview;
    }
    save(&cfg)
}

/// Live partial transcription during recording; on by default.
pub fn realtime_preview() -> bool {
    load().realtime_preview.unwrap_or(true)
}

/// STA agent id; default `omp`.
pub fn sta_agent_id() -> String {
    match load().sta_agent.as_deref() {
        Some(a @ ("omp" | "claude" | "codex" | "custom")) => a.to_string(),
        _ => "omp".to_string(),
    }
}

/// Shell command template for STA. `{prompt}` is replaced by the dictated
/// request. Empty = built-in preset for the agent.
pub fn sta_command_template() -> String {
    load().sta_command.filter(|s| !s.trim().is_empty()).unwrap_or_default()
}

/// Default working dir for the STA agent terminal (`sta_cwd` setting,
/// `~/Projets` when unset). `~/` prefixes are expanded.
pub fn sta_cwd_default() -> String {
    let raw = load().sta_cwd.filter(|s| !s.trim().is_empty());
    let expand = |s: String| {
        if s.starts_with("~/") {
            match dirs::home_dir() {
                Some(h) => format!("{}{}", h.display(), &s[1..]),
                None => s,
            }
        } else {
            s
        }
    };
    match raw {
        Some(s) => expand(s),
        None => dirs::home_dir()
            .map(|h| h.join("Projets").to_string_lossy().into_owned())
            .unwrap_or_else(|| "/tmp".to_string()),
    }
}

/// Screen+folder context joined to the STA prompt; on by default.
pub fn sta_context_enabled() -> bool {
    load().sta_context.unwrap_or(true)
}

/// Preferred capture device name (cpal name); None = system default.
pub fn input_device_name() -> Option<String> {
    load().input_device.filter(|s| !s.trim().is_empty())
}

/// Dictation language sent to STT; default `fr` (no auto-detect cost, no
/// sticky-lang surprise). `auto` restores detection, `en` forces English.
pub fn dictation_language() -> String {
    match load().language.as_deref() {
        Some(l @ ("fr" | "en" | "auto")) => l.to_string(),
        _ => "fr".to_string(),
    }
}

/// Validated local model name; default is `medium` (FR quality), `small`/`base` for speed.
pub fn stt_model_name() -> String {
    match load().stt_model.as_deref() {
        Some(m @ ("small" | "medium" | "base")) => m.to_string(),
        _ => "medium".to_string(),
    }
}
