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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_paste: Option<bool>,
    /// Pills actives (Settings → Pills) : ids parmi `dictee`, `sta`, `agents`, `chat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pills: Option<Vec<String>>,
    /// Adresse du serveur Ollama local (`Modèles locaux`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_url: Option<String>,
    /// Modèle Ollama par défaut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_model: Option<String>,
    /// Fournisseur de chat actif (Settings → Chat providers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_provider: Option<String>,
    /// Modèle du fournisseur de chat actif.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_model: Option<String>,
    /// Base URL OpenAI-compatible (DeepSeek/Qwen/Moonshot/serveur custom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_base_url: Option<String>,
    /// Voix Fish pour `speak_text` (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fish_voice: Option<String>,
    /// Voix STA on/off (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sta_voice: Option<bool>,
    /// Taille popup S/M/L (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_size: Option<String>,
    /// Opacité carte (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_opacity: Option<f64>,
    /// Mochi popup on/off (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_mochi: Option<bool>,
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
    auto_paste: Option<bool>,
    active_pills: Option<Vec<String>>,
    ollama_url: Option<String>,
    ollama_model: Option<String>,
    chat_provider: Option<String>,
    chat_model: Option<String>,
    chat_base_url: Option<String>,
    fish_voice: Option<String>,
    sta_voice: Option<bool>,
    popup_size: Option<String>,
    card_opacity: Option<f64>,
    popup_mochi: Option<bool>,
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
    if auto_paste.is_some() {
        cfg.auto_paste = auto_paste;
    }
    if active_pills.is_some() {
        cfg.active_pills = active_pills;
    }
    if ollama_url.is_some() {
        cfg.ollama_url = ollama_url;
    }
    if ollama_model.is_some() {
        cfg.ollama_model = ollama_model;
    }
    if let Some(p) = &chat_provider {
        const KNOWN: &[&str] = &[
            "ollama", "anthropic", "openai", "google", "openrouter", "deepseek", "qwen", "moonshot",
            "custom",
        ];
        if !KNOWN.contains(&p.as_str()) {
            return Err(format!("unknown chat_provider '{p}'"));
        }
    }
    if chat_provider.is_some() {
        cfg.chat_provider = chat_provider;
    }
    if chat_model.is_some() {
        cfg.chat_model = chat_model;
    }
    if chat_base_url.is_some() {
        cfg.chat_base_url = chat_base_url;
    }
    // Champs WhispSettings2 : sans validation, assignés si présents.
    if fish_voice.is_some() {
        cfg.fish_voice = fish_voice;
    }
    if sta_voice.is_some() {
        cfg.sta_voice = sta_voice;
    }
    if popup_size.is_some() {
        cfg.popup_size = popup_size;
    }
    if card_opacity.is_some() {
        cfg.card_opacity = card_opacity;
    }
    if popup_mochi.is_some() {
        cfg.popup_mochi = popup_mochi;
    }
    save(&cfg)
}

/// Auto-paste the final text at the cursor (Ctrl+V); on by default.
/// Off = clipboard only (+ history).
pub fn auto_paste() -> bool {
    load().auto_paste.unwrap_or(true)
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

/// Pills actives (Settings → Pills). Défaut : les 4 affichées.
pub fn active_pills() -> Vec<String> {
    let all = vec![
        "dictee".to_string(),
        "sta".to_string(),
        "agents".to_string(),
        "chat".to_string(),
    ];
    match load().active_pills {
        Some(list) => {
            let kept: Vec<String> = list
                .into_iter()
                .filter(|p| all.iter().any(|a| a == p))
                .collect();
            if kept.is_empty() { all } else { kept }
        }
        None => all,
    }
}

/// Adresse du serveur Ollama ; défaut boucle locale.
pub fn ollama_url() -> String {
    load()
        .ollama_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string())
}

/// Modèle Ollama par défaut.
pub fn ollama_model() -> String {
    load()
        .ollama_model
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "qwen2.5:3b".to_string())
}
