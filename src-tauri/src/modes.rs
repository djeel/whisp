//! Dictation modes (presets + user customs) for whisp.
//!
//! A [`Mode`] selects the STT engine, the LLM refiner, the language and an
//! extra vocabulary. Six builtins mirror Superwhisper behaviour; user modes
//! persist as JSON in `~/.config/whisp/modes.json` (builtins are never
//! written, customs override by id).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttKind {
    Local,
    CloudFishAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmKind {
    /// No refinement: raw transcript is final (lowest latency).
    None,
    /// Local Ollama (`qwen2.5:3b`).
    OllamaLocal,
    /// Anthropic API via keyring key.
    CloudAnthropic,
    /// OpenAI-compatible API via keyring key.
    CloudOpenAi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mode {
    pub id: String,
    pub label: String,
    pub preset: String,
    pub prompt: String,
    pub stt: SttKind,
    pub llm: LlmKind,
    pub language: String,
    #[serde(default)]
    pub vocabulary: Vec<String>,
}

impl Mode {
    fn builtin(
        id: &str,
        label: &str,
        preset: &str,
        prompt: &str,
        stt: SttKind,
        llm: LlmKind,
        language: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            preset: preset.to_string(),
            prompt: prompt.to_string(),
            stt,
            llm,
            language: language.to_string(),
            vocabulary: Vec::new(),
        }
    }
}

/// The six builtin presets.
pub fn builtin_modes() -> Vec<Mode> {
    vec![
        Mode::builtin(
            "message",
            "Message",
            "message",
            "Rewrite the transcript as a short, clear chat message in the SAME language as the transcript (never translate). Fix grammar and punctuation, keep the tone casual, no greeting, no quotes. Reply with the message only.",
            SttKind::CloudFishAudio,
            LlmKind::OllamaLocal,
            "auto",
        ),
        Mode::builtin(
            "super",
            "Super",
            "super",
            "Rewrite the transcript into polished, high-quality prose in the SAME language as the transcript (never translate). Fix grammar, structure and flow while preserving meaning. Reply with the rewritten text only.",
            SttKind::Local,
            LlmKind::OllamaLocal,
            "auto",
        ),
        Mode::builtin(
            "mail",
            "Mail",
            "mail",
            "Rewrite the transcript as a professional email body in the SAME language as the transcript (never translate): clear subject-free paragraphs, polite tone, correct grammar. Reply with the email body only.",
            SttKind::Local,
            LlmKind::OllamaLocal,
            "auto",
        ),
        Mode::builtin(
            "note",
            "Note",
            "note",
            "Turn the transcript into a concise note in the SAME language as the transcript (never translate): keep every fact, drop filler words, use short bullet-style lines when it helps. Reply with the note only.",
            SttKind::Local,
            LlmKind::None,
            "auto",
        ),
        Mode::builtin(
            "voice",
            "Voice",
            "voice",
            "Minimal cleanup only, SAME language as the transcript (never translate): fix obvious transcription mistakes and add punctuation. Keep the speaker's exact words and style otherwise. Reply with the text only.",
            SttKind::Local,
            LlmKind::None,
            "auto",
        ),
        Mode::builtin(
            "meeting",
            "Meeting",
            "meeting",
            "The transcript is a multi-speaker meeting. Produce short minutes: decisions, action items (with owner when named), key points. Language: same as transcript. Reply with the minutes only.",
            SttKind::CloudFishAudio,
            LlmKind::OllamaLocal,
            "auto",
        ),
        Mode::builtin(
            "sta",
            "STA",
            "sta",
            "Speech to agent: keep the raw request verbatim, only add punctuation. The launcher attaches folder/screenshot context itself; keep the request untouched. Reply with the request only.",
            SttKind::Local,
            LlmKind::None,
            "auto",
        ),
    ]
}

// ---------------------------------------------------------------------------
// Store: builtins merged with ~/.config/whisp/modes.json customs
// ---------------------------------------------------------------------------

fn customs_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("whisp")
        .join("modes.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CustomsFile {
    #[serde(default)]
    modes: Vec<Mode>,
}

fn load_customs() -> Vec<Mode> {
    std::fs::read_to_string(customs_path())
        .ok()
        .and_then(|s| serde_json::from_str::<CustomsFile>(&s).ok())
        .map(|f| f.modes)
        .unwrap_or_default()
}

fn save_customs(modes: &[Mode]) -> Result<(), String> {
    let path = customs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir modes: {e}"))?;
    }
    let file = CustomsFile {
        modes: modes.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("encode modes: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write modes: {e}"))
}

/// All modes: builtins overlaid by customs (custom wins on id clash).
pub fn list_modes() -> Vec<Mode> {
    let mut map: HashMap<String, Mode> = builtin_modes().into_iter().map(|m| (m.id.clone(), m)).collect();
    for m in load_customs() {
        map.insert(m.id.clone(), m);
    }
    let mut out: Vec<Mode> = map.into_values().collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn get_mode(id: &str) -> Option<Mode> {
    list_modes().into_iter().find(|m| m.id == id)
}

/// Create or replace a custom mode. Builtin ids may be shadowed (stored in
/// the customs file, builtins themselves are untouched).
pub fn upsert_custom_mode(mode: Mode) -> Result<Mode, String> {
    validate_mode(&mode)?;
    let mut customs = load_customs();
    if let Some(slot) = customs.iter_mut().find(|m| m.id == mode.id) {
        *slot = mode.clone();
    } else {
        customs.push(mode.clone());
    }
    save_customs(&customs)?;
    Ok(mode)
}

/// Delete a custom mode. Deleting a builtin id removes the custom shadow.
pub fn delete_custom_mode(id: &str) -> Result<(), String> {
    let mut customs = load_customs();
    let before = customs.len();
    customs.retain(|m| m.id != id);
    if customs.len() == before {
        return Err(format!("no custom mode '{id}'"));
    }
    save_customs(&customs)
}

fn validate_mode(mode: &Mode) -> Result<(), String> {
    if mode.id.trim().is_empty() {
        return Err("mode id is empty".to_string());
    }
    if mode.id.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        return Err("mode id must be [a-zA-Z0-9_-]".to_string());
    }
    if mode.label.trim().is_empty() {
        return Err("mode label is empty".to_string());
    }
    Ok(())
}

/// Apply the per-mode extra vocabulary: append missing terms to the prompt
/// so the LLM keeps spellings, plus return them for post-fixes.
pub fn vocabulary_hint(mode: &Mode) -> String {
    if mode.vocabulary.is_empty() {
        return String::new();
    }
    format!(
        " Domain terms (keep exact spelling): {}.",
        mode.vocabulary.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_builtins_present() {
        let ids: Vec<String> = builtin_modes().into_iter().map(|m| m.id).collect();
        for want in ["message", "super", "mail", "note", "voice", "meeting"] {
            assert!(ids.contains(&want.to_string()), "missing {want}");
        }
    }
}
