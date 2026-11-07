//! App config for whisp (`~/.config/whisp/config.json`).
//!
//! Small JSON file shared between backend and frontend: local STT model
//! (`medium` by default, `small`/`base` for speed), recorder popup position and
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
    // Note: pre-rename keys are migrated in `load()` below (one-version compat).
    /// Preset agent invoqué dans la session tmux `whisp-agent` :
    /// `omp` (défaut) | `claude` | `codex` | `custom`. Tout autre id retombe
    /// sur `omp` ; un preset dont le binaire est absent retombe aussi sur
    /// `omp` (jamais de terminal qui meurt à l'ouverture).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Template shell custom quand `agent = "custom"` — `{prompt}` est
    /// remplacé par la demande quotée shell. Vide = preset intégré.
    /// Ex. `mon-agent --task {prompt} --yes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_command: Option<String>,
    /// Dossier de travail de la session agent (`~/` étendu, défaut
    /// `~/Projets`). La dictée y `cd` avant chaque injection, même si le
    /// shell de la session s'est déplacé ailleurs entre deux demandes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_cwd: Option<String>,
    /// Contexte dossier + capture d'écran joints au prompt (`Contexte:
    /// dossier <nom> (<chemin>). [Capture d'écran: <fichier>.] Demande:
    /// …`, demande tronquée à ~4000 car). On par défaut ; off = transcript brut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realtime_preview: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_paste: Option<bool>,
    /// Pills actives (Settings → Pills) : ids parmi `dictee`, `agent`, `agents`, `chat`.
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
    /// Style Claude (ton préréglé, ex. Concise) : injecté en `system`
    /// pour le provider `claude` (chat + raffinage) et Anthropic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_style: Option<String>,
    /// Voix Fish pour `speak_text` (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fish_voice: Option<String>,
    /// Voix Agent on/off (WhispSettings2) : lit la demande à voix haute (Fish TTS) après injection dans tmux. Off par défaut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_voice: Option<bool>,
    /// Taille popup S/M/L (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_size: Option<String>,
    /// Opacité carte (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_opacity: Option<f64>,
    /// Mochi popup on/off (WhispSettings2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_mochi: Option<bool>,
    /// Source de l'accent du recorder (`Apparence`) : `theme` (défaut,
    /// suit le clair/sombre) | `custom` (`accent_custom`) | `caelestia`
    /// (scheme Caelestia, suit le fond d'écran).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_source: Option<String>,
    /// Accent personnalisé `#rrggbb` (quand `accent_source = "custom"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_custom: Option<String>,
    /// Raccourci toggle dictée, format `alt+Space`. Vide/absent = défaut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_toggle: Option<String>,
    /// Raccourci push-to-talk (maintenir), format `control+alt+Space`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_push_to_talk: Option<String>,
    /// Raccourci cycle de mode, format `control+alt+M`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_cycle_mode: Option<String>,
    /// Position mémorisée de la popup "x,y" logiques (drag utilisateur).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup_xy: Option<String>,
}
fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join(".config")
        })
        .join("whisp/config.json")
}

/// Verrou global config : `set()` = load + merge + save atomique.
/// Sans lui, deux `set_config` concurrents (mount Settings + dropdown)
/// lisent la même version et le second écrase le premier (lost update) ;
/// pire, deux `write` simultanés = fichier tronqué = carte coupée au reload.
static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn load() -> AppConfig {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked()
}

fn load_unlocked() -> AppConfig {
    let text = std::fs::read_to_string(config_path()).unwrap_or_default();
    let raw: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let mut cfg: AppConfig = serde_json::from_value(raw.clone()).unwrap_or_default();
    // Compat: pre-rename keys apply only when the new key is absent (one version).
    if cfg.agent.is_none() {
        cfg.agent = raw
            .get("staAgent")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    } // Compat sta
    if cfg.agent_command.is_none() {
        cfg.agent_command = raw
            .get("staCommand")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    } // Compat sta
    if cfg.agent_cwd.is_none() {
        cfg.agent_cwd = raw
            .get("staCwd")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    } // Compat sta
    if cfg.agent_context.is_none() {
        cfg.agent_context = raw.get("staContext").and_then(|x| x.as_bool());
    } // Compat sta
    if cfg.agent_voice.is_none() {
        cfg.agent_voice = raw.get("staVoice").and_then(|x| x.as_bool());
    } // Compat sta
    cfg
}

/// Accept only `http(s)://host...` URLs. Rejects `file://`, bare hostnames
/// and whitespace so a bad paste can't turn the local Ollama/chat calls
/// into SSRF. Bloque aussi les metadata cloud link-local (169.254.169.254,
/// 100.100.100.200, metadata.google.internal) : loopback reste autorisé
/// (Ollama local tourne en 127.0.0.1).
fn blocked_metadata_host(host: &str) -> bool {
    let h = host.trim().trim_matches(['[', ']']).to_lowercase();
    h == "169.254.169.254"
        || h.starts_with("169.254.")
        || h == "100.100.100.200"
        || h == "metadata.google.internal"
        || h == "metadata.google"
        || h == "instance-data"
}

fn validate_http_url(field: &str, v: &str) -> Result<(), String> {
    let v = v.trim();
    let rest = v
        .strip_prefix("http://")
        .or_else(|| v.strip_prefix("https://"))
        .ok_or_else(|| format!("{field} must start with http:// or https:// (got '{v}')"))?;
    if rest.is_empty() || v.contains(char::is_whitespace) {
        return Err(format!("{field} needs an http(s) URL with a host (got '{v}')"));
    }
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("");
    if host.is_empty() {
        return Err(format!("{field} needs an http(s) URL with a host (got '{v}')"));
    }
    if blocked_metadata_host(host) {
        return Err(format!("{field} metadata cloud bloquée (got '{v}')"));
    }
    Ok(())
}

/// Merge a partial update into the stored file. `stt_model` is validated
/// (`small`/`medium`/`base`); other fields are opaque UI preferences.
pub fn set(
    popup_pos: Option<String>,
    stt_model: Option<String>,
    theme: Option<String>,
    language: Option<String>,
    input_device: Option<String>,
    agent: Option<String>,
    agent_command: Option<String>,
    agent_cwd: Option<String>,
    agent_context: Option<bool>,
    realtime_preview: Option<bool>,
    auto_paste: Option<bool>,
    active_pills: Option<Vec<String>>,
    ollama_url: Option<String>,
    ollama_model: Option<String>,
    chat_provider: Option<String>,
    chat_model: Option<String>,
    chat_base_url: Option<String>,
    claude_style: Option<String>,
    fish_voice: Option<String>,
    agent_voice: Option<bool>,
    popup_size: Option<String>,
    card_opacity: Option<f64>,
    popup_mochi: Option<bool>,
    popup_xy: Option<String>,
    accent_source: Option<String>,
    accent_custom: Option<String>,
    hotkey_toggle: Option<String>,
    hotkey_push_to_talk: Option<String>,
    hotkey_cycle_mode: Option<String>,
) -> Result<(), String> {
    if let Some(m) = &stt_model {
        if !matches!(m.as_str(), "small" | "medium" | "base") {
            return Err(format!("unknown stt_model '{m}' (small|medium|base)"));
        }
    }
    if let Some(s) = &accent_source {
        if !matches!(s.as_str(), "theme" | "custom" | "caelestia") {
            return Err(format!(
                "unknown accent_source '{s}' (theme|custom|caelestia)"
            ));
        }
    }
    if let Some(c) = &accent_custom {
        let ok =
            c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
        if !ok {
            return Err(format!("unknown accent_custom '{c}' (#rrggbb)"));
        }
    }
    // Section critique : load + merge + save sous le même verrou.
    // Évite lost-update et fichier tronqué (deux write simultanés).
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut cfg = load_unlocked();
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
    if let Some(a) = &agent {
        if !matches!(a.as_str(), "omp" | "claude" | "codex" | "custom") {
            return Err(format!("unknown agent '{a}' (omp|claude|codex|custom)"));
        }
    }
    if agent.is_some() {
        cfg.agent = agent;
    }
    if agent_command.is_some() {
        cfg.agent_command = agent_command;
    }
    if agent_cwd.is_some() {
        cfg.agent_cwd = agent_cwd;
    }
    if agent_context.is_some() {
        cfg.agent_context = agent_context;
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
    if let Some(u) = &ollama_url {
        validate_http_url("ollama_url", u)?;
    }
    if ollama_url.is_some() {
        cfg.ollama_url = ollama_url;
    }
    if ollama_model.is_some() {
        cfg.ollama_model = ollama_model;
    }
    if let Some(p) = &chat_provider {
        const KNOWN: &[&str] = &[
            "ollama",
            "anthropic",
            "claude",
            "openai",
            "google",
            "openrouter",
            "deepseek",
            "qwen",
            "moonshot",
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
    if let Some(u) = &chat_base_url {
        validate_http_url("chat_base_url", u)?;
    }
    if chat_base_url.is_some() {
        cfg.chat_base_url = chat_base_url;
    }
    if claude_style.is_some() {
        cfg.claude_style = claude_style;
    }
    // Champs WhispSettings2 : sans validation, assignés si présents.
    if fish_voice.is_some() {
        cfg.fish_voice = fish_voice;
    }
    if agent_voice.is_some() {
        cfg.agent_voice = agent_voice;
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
    if popup_xy.is_some() {
        cfg.popup_xy = popup_xy;
    }
    if accent_source.is_some() {
        cfg.accent_source = accent_source;
    }
    if accent_custom.is_some() {
        cfg.accent_custom = accent_custom;
    }
    if hotkey_toggle.is_some() {
        cfg.hotkey_toggle = hotkey_toggle;
    }
    if hotkey_push_to_talk.is_some() {
        cfg.hotkey_push_to_talk = hotkey_push_to_talk;
    }
    if hotkey_cycle_mode.is_some() {
        cfg.hotkey_cycle_mode = hotkey_cycle_mode;
    }
    // Écriture atomique : tmp + rename (jamais de JSON à moitié écrit,
    // même en cas de kill pendant le write).
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("config dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("config encode: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("config write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("config rename: {e}"))
}

/// Auto-paste the final text at the cursor (Ctrl+V); on by default.
/// Off = clipboard only (+ history).
pub fn auto_paste() -> bool {
    load().auto_paste.unwrap_or(true)
}

/// Position mémorisée "x,y" (drag utilisateur), None si jamais draggée.
pub fn popup_xy() -> Option<String> {
    load().popup_xy.filter(|s| !s.trim().is_empty())
}

/// Live partial transcription during recording; on by default.
pub fn realtime_preview() -> bool {
    load().realtime_preview.unwrap_or(true)
}

/// Agent id; default `omp`.
pub fn agent_id() -> String {
    match load().agent.as_deref() {
        Some(a @ ("omp" | "claude" | "codex" | "custom")) => a.to_string(),
        _ => "omp".to_string(),
    }
}

/// Shell command template for the Agent mode. `{prompt}` is replaced by the dictated
/// request. Empty = built-in preset for the agent.
pub fn agent_command_template() -> String {
    load()
        .agent_command
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_default()
}

/// Default working dir for the Agent terminal (`agent_cwd` setting,
/// `~/Projets` when unset). `~/` prefixes are expanded.
pub fn agent_cwd_default() -> String {
    let raw = load().agent_cwd.filter(|s| !s.trim().is_empty());
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

/// Screen+folder context joined to the Agent prompt; on by default.
pub fn agent_context_enabled() -> bool {
    load().agent_context.unwrap_or(true)
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

/// Validated local model name; default is `small` (mesuré : 2.2 s vs 7.4 s
/// pour `medium` sur 7.8 s FR, même texte — `medium` opt-in via Settings).
pub fn stt_model_name() -> String {
    match load().stt_model.as_deref() {
        Some(m @ ("small" | "medium" | "base")) => m.to_string(),
        _ => "small".to_string(),
    }
}

/// Pills actives (Settings → Pills). Défaut : les 4 affichées.
pub fn active_pills() -> Vec<String> {
    let all = vec![
        "dictee".to_string(),
        "agent".to_string(),
        "agents".to_string(),
        "chat".to_string(),
    ];
    match load().active_pills {
        Some(list) => {
            let kept: Vec<String> = list
                .into_iter()
                .filter(|p| all.iter().any(|a| a == p))
                .collect();
            if kept.is_empty() {
                all
            } else {
                kept
            }
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
        .unwrap_or_else(|| "whisp-nanbeige".to_string())
}

/// Source de l'accent recorder ; défaut `theme`.
pub fn accent_source_name() -> String {
    match load().accent_source.as_deref() {
        Some(s @ ("theme" | "custom" | "caelestia")) => s.to_string(),
        _ => "theme".to_string(),
    }
}

/// Accent personnalisé `#rrggbb` ; défaut neutre (bouton Stop actuel).
pub fn accent_custom_hex() -> String {
    match load().accent_custom.as_deref() {
        Some(c)
            if c.len() == 7
                && c.starts_with('#')
                && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) =>
        {
            c.to_string()
        }
        _ => "#f59e0b".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);
    /// `std::env::set_var` est global au process (tests en parallèle) :
    /// sérialise toute isolation `XDG_CONFIG_HOME`, sinon deux tests
    /// déplacent le fichier config l'un sous l'autre (flaky).
    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Isole le fichier config réel : `dirs::config_dir()` suit `XDG_CONFIG_HOME`
    /// sur Linux, donc chaque test pointe vers un tmpdir jetable.
    /// Raison : sans ça, `cargo test` écrirait dans `~/.config/whisp/`.
    fn with_isolated_config(f: impl FnOnce()) {
        let _lock = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let id = TEST_DIR_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("whisp-cfg-test-{}-{}", std::process::id(), id));
        let _ = std::fs::create_dir_all(&dir);
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        f();
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `set` prend 26 params positionnels (même ordre que la struct).
    /// Helpers : un seul champ `Some`, le reste `None` — comptage vérifiable
    /// ligne à ligne (12 + 12 + 2).
    fn set_popup_pos(v: String) -> Result<(), String> {
        set(
            Some(v),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
    fn set_theme(v: String) -> Result<(), String> {
        set(
            None,
            None,
            Some(v),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
    fn set_language_fr() -> Result<(), String> {
        set(
            None,
            None,
            None,
            Some("fr".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
    fn set_agent_cwd(v: String) -> Result<(), String> {
        set(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(v),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
    fn set_accent(source: Option<&str>, custom: Option<&str>) -> Result<(), String> {
        set(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            source.map(|s| s.to_string()),
            custom.map(|s| s.to_string()),
            None,
            None,
            None,
        )
    }

    #[test]
    fn accent_validation_rejects_bad_values() {
        // `set` valide AVANT le lock et avant tout IO : aucun fichier touché
        // quand la valeur est rejetée (test sûr, sans isolation).
        assert!(set_accent(Some("nope"), None).is_err());
        assert!(set_accent(Some("custom"), Some("#zzzzzz")).is_err());
        assert!(set_accent(Some("custom"), Some("a9cae8")).is_err()); // `#` requis
        assert!(set_accent(Some("custom"), Some("#a9cae8ff")).is_err()); // 6 hex, pas 8
    }

    #[test]
    fn accent_validation_accepts_good_hex() {
        // Le cas valide écrit : isolé pour ne pas toucher la vraie config.
        with_isolated_config(|| {
            assert!(set_accent(Some("custom"), Some("#a9cae8")).is_ok());
            assert_eq!(accent_custom_hex(), "#a9cae8");
            assert_eq!(accent_source_name(), "custom");
        });
    }

    #[test]
    fn url_validation_rejects_non_http() {
        // `set` valide AVANT le lock : aucun fichier touché quand rejeté.
        assert!(validate_http_url("ollama_url", "file:///etc/passwd").is_err());
        assert!(validate_http_url("ollama_url", "localhost:11434").is_err());
        assert!(validate_http_url("ollama_url", "http://").is_err());
        assert!(validate_http_url("ollama_url", "https://ollama local/v1").is_err());
        assert!(validate_http_url("ollama_url", "http://127.0.0.1:11434").is_ok());
        assert!(validate_http_url("chat_base_url", "https://api.openai.com/v1").is_ok());
        // Metadata cloud bloquée, loopback OK pour Ollama local.
        assert!(validate_http_url("chat_base_url", "http://169.254.169.254/latest").is_err());
        assert!(validate_http_url("ollama_url", "http://100.100.100.200/").is_err());
    }

    #[test]
    fn concurrent_partial_sets_do_not_lose_updates() {
        // Lost-update : 8 threads écrivent chacun un champ disjoint via
        // load+merge+save sous `CONFIG_LOCK`. Sans le verrou, le dernier
        // `write` écraserait les 7 autres ; avec, chaque champ survit.
        with_isolated_config(|| {
            let _ = std::fs::remove_file(config_path());
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    std::thread::spawn(move || {
                        let v = format!("v{i}");
                        // Chaque thread ne touche qu'un champ : merge disjoint.
                        let r = match i % 4 {
                            0 => set_popup_pos(v),
                            1 => set_theme(v),
                            2 => set_language_fr(),
                            _ => set_agent_cwd(v),
                        };
                        assert!(r.is_ok(), "set thread {i}: {r:?}");
                    })
                })
                .collect();
            for h in handles {
                h.join().expect("set thread panicked");
            }
            // Fichier jamais tronqué : JSON valide après 8 writes concurrents
            // (tmp+rename : pas de demi-fichier observable).
            let raw = std::fs::read_to_string(config_path()).expect("config file exists");
            let val: serde_json::Value =
                serde_json::from_str(&raw).expect("valid JSON, never truncated");
            assert!(val.is_object());
            let cfg = load();
            // Chaque famille disjointe a survécu au merge : aucun lost update.
            assert!(
                cfg.popup_pos.as_deref().is_some_and(|s| s.starts_with('v')),
                "popup_pos: {:?}",
                cfg.popup_pos
            );
            assert!(
                cfg.theme.as_deref().is_some_and(|s| s.starts_with('v')),
                "theme: {:?}",
                cfg.theme
            );
            assert_eq!(cfg.language.as_deref(), Some("fr"));
            assert!(
                cfg.agent_cwd.as_deref().is_some_and(|s| s.starts_with('v')),
                "agent_cwd: {:?}",
                cfg.agent_cwd
            );
        });
    }
}
