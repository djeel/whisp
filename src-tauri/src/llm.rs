//! LLM refinement for whisp.
//!
//! [`Llm::refine`] polishes a raw transcript according to the active mode.
//! - [`OllamaLocal`]: `qwen2.5:3b` on `http://localhost:11434` (offline).
//! - [`CloudAnthropic`] / [`CloudOpenAi`]: via `reqwest`, keys from the OS
//!   keyring — no network call without a configured key.

use serde::{Deserialize, Serialize};

use crate::modes::{LlmKind, Mode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Refined {
    pub text: String,
    pub engine: String,
}

pub trait Llm: Send + Sync {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String>;
}

// ---------------------------------------------------------------------------
// Shared prompt builder
// ---------------------------------------------------------------------------

pub fn build_prompt(transcript: &str, mode: &Mode) -> String {
    let vocab = crate::modes::vocabulary_hint(mode);
    format!(
        "{}{}\n\nTranscript:\n\"\"\"\n{}\n\"\"\"",
        mode.prompt,
        vocab,
        transcript.trim()
    )
}

// ---------------------------------------------------------------------------
// Local: Ollama qwen2.5:3b
// ---------------------------------------------------------------------------

pub struct OllamaLocal {
    pub base_url: String,
    pub model: String,
    pub client: reqwest::blocking::Client,
}

impl Default for OllamaLocal {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "qwen2.5:3b".to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

impl Llm for OllamaLocal {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        if transcript.trim().is_empty() {
            return Ok(Refined {
                text: String::new(),
                engine: self.model.clone(),
            });
        }
        let prompt = build_prompt(transcript, mode);
        let resp: OllamaResponse = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&OllamaRequest {
                model: &self.model,
                prompt: &prompt,
                stream: false,
            })
            .send()
            .map_err(|e| format!("ollama ({}) unreachable — is `ollama serve` running?: {e}", self.base_url))?
            .error_for_status()
            .map_err(|e| format!("ollama status: {e}"))?
            .json()
            .map_err(|e| format!("ollama decode: {e}"))?;
        Ok(Refined {
            text: resp.response.trim().to_string(),
            engine: format!("ollama:{}", self.model),
        })
    }
}

// ---------------------------------------------------------------------------
// Cloud: Anthropic / OpenAI-compatible
// ---------------------------------------------------------------------------

fn keyring_get(user: &str) -> Option<String> {
    keyring::Entry::new("whisp", user)
        .ok()?
        .get_password()
        .ok()
        .filter(|k| !k.trim().is_empty())
}

pub fn set_cloud_key(provider: &str, key: &str) -> Result<(), String> {
    keyring::Entry::new("whisp", provider)
        .map_err(|e| format!("keyring: {e}"))?
        .set_password(key)
        .map_err(|e| format!("keyring: {e}"))
}

pub struct CloudAnthropic {
    pub model: String,
    pub client: reqwest::blocking::Client,
}

impl Default for CloudAnthropic {
    fn default() -> Self {
        Self {
            model: "claude-haiku-4-5".to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl Llm for CloudAnthropic {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        let key = keyring_get("anthropic").ok_or_else(|| "anthropic key not configured".to_string())?;
        let prompt = build_prompt(transcript, mode);
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let resp: serde_json::Value = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .map_err(|e| format!("anthropic request: {e}"))?
            .error_for_status()
            .map_err(|e| format!("anthropic status: {e}"))?
            .json()
            .map_err(|e| format!("anthropic decode: {e}"))?;
        let text = resp
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        Ok(Refined {
            text,
            engine: format!("anthropic:{}", self.model),
        })
    }
}

pub struct CloudOpenAi {
    pub model: String,
    pub base_url: String,
    pub client: reqwest::blocking::Client,
}

impl Default for CloudOpenAi {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl Llm for CloudOpenAi {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        let key = keyring_get("openai").ok_or_else(|| "openai key not configured".to_string())?;
        let prompt = build_prompt(transcript, mode);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {key}"))
            .json(&body)
            .send()
            .map_err(|e| format!("openai request: {e}"))?
            .error_for_status()
            .map_err(|e| format!("openai status: {e}"))?
            .json()
            .map_err(|e| format!("openai decode: {e}"))?;
        let text = resp
            .pointer("/choices/0/message/content")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        Ok(Refined {
            text,
            engine: format!("openai:{}", self.model),
        })
    }
}

// ---------------------------------------------------------------------------
// Chat tab: blocking single-turn completion (no streaming)
// ---------------------------------------------------------------------------

/// One chat message exchanged with the Chat tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

fn chat_role(role: &str) -> &str {
    match role {
        "assistant" => "assistant",
        _ => "user",
    }
}

/// Blocking chat completion. Cloud providers require a keyring key (no
/// network call without one); `ollama` stays local (no key needed).
/// `messages` are the last N turns; the reply text is returned.
pub fn chat(provider: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    match provider {
        "anthropic" => {
            let key = keyring_get("anthropic").ok_or_else(|| "anthropic key not configured".to_string())?;
            let system: Vec<&str> = messages
                .iter()
                .filter(|m| m.role == "system")
                .map(|m| m.content.as_str())
                .collect();
            let turns: Vec<serde_json::Value> = messages
                .iter()
                .filter(|m| m.role != "system")
                .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
                .collect();
            if turns.is_empty() {
                return Err("empty conversation".to_string());
            }
            let mut body = serde_json::json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 1024,
                "messages": turns,
            });
            if !system.is_empty() {
                body["system"] = serde_json::Value::String(system.join("\n"));
            }
            let resp: serde_json::Value = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .map_err(|e| format!("anthropic request: {e}"))?
                .error_for_status()
                .map_err(|e| format!("anthropic status: {e}"))?
                .json()
                .map_err(|e| format!("anthropic decode: {e}"))?;
            Ok(resp
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|b| b.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .trim()
                .to_string())
        }
        "openai" => {
            let key = keyring_get("openai").ok_or_else(|| "openai key not configured".to_string())?;
            let turns: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
                .collect();
            if turns.is_empty() {
                return Err("empty conversation".to_string());
            }
            let resp: serde_json::Value = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {key}"))
                .json(&serde_json::json!({ "model": "gpt-4o-mini", "messages": turns }))
                .send()
                .map_err(|e| format!("openai request: {e}"))?
                .error_for_status()
                .map_err(|e| format!("openai status: {e}"))?
                .json()
                .map_err(|e| format!("openai decode: {e}"))?;
            Ok(resp
                .pointer("/choices/0/message/content")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .trim()
                .to_string())
        }
        _ => {
            // Default: local Ollama `/api/chat` (offline, no key).
            let turns: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
                .collect();
            if turns.is_empty() {
                return Err("empty conversation".to_string());
            }
            let resp: serde_json::Value = client
                .post("http://localhost:11434/api/chat")
                .json(&serde_json::json!({
                    "model": "qwen2.5:3b",
                    "messages": turns,
                    "stream": false,
                }))
                .send()
                .map_err(|e| format!("ollama unreachable — is `ollama serve` running?: {e}"))?
                .error_for_status()
                .map_err(|e| format!("ollama status: {e}"))?
                .json()
                .map_err(|e| format!("ollama decode: {e}"))?;
            Ok(resp
                .pointer("/message/content")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .trim()
                .to_string())
        }
    }
}

/// Tauri command: blocking chat completion for the Chat tab.
#[tauri::command]
pub fn chat_command(messages: Vec<ChatMessage>, provider: String) -> Result<String, String> {
    chat(&provider, &messages)
}

/// Tauri command: aggregate dictation stats from `history.db`.
#[tauri::command]
pub fn dictation_stats() -> Result<crate::history::DictationStats, String> {
    crate::history::stats()
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

pub enum SelectedLlm {
    Passthrough,
    Ollama(OllamaLocal),
    Anthropic(CloudAnthropic),
    OpenAi(CloudOpenAi),
}

impl Llm for SelectedLlm {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        match self {
            SelectedLlm::Passthrough => Ok(Refined {
                text: transcript.trim().to_string(),
                engine: "none".to_string(),
            }),
            SelectedLlm::Ollama(l) => l.refine(transcript, mode),
            SelectedLlm::Anthropic(l) => l.refine(transcript, mode),
            SelectedLlm::OpenAi(l) => l.refine(transcript, mode),
        }
    }
}

/// No network unless a key is configured: cloud kinds without keys fall
/// back to local Ollama (or passthrough when Ollama is unreachable — the
/// refine call itself reports that error, never a surprise cloud call).
pub fn select_llm(kind: LlmKind) -> SelectedLlm {
    match kind {
        LlmKind::None => SelectedLlm::Passthrough,
        LlmKind::OllamaLocal => SelectedLlm::Ollama(OllamaLocal::default()),
        LlmKind::CloudAnthropic => {
            if keyring_get("anthropic").is_some() {
                SelectedLlm::Anthropic(CloudAnthropic::default())
            } else {
                SelectedLlm::Ollama(OllamaLocal::default())
            }
        }
        LlmKind::CloudOpenAi => {
            if keyring_get("openai").is_some() {
                SelectedLlm::OpenAi(CloudOpenAi::default())
            } else {
                SelectedLlm::Ollama(OllamaLocal::default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::SttKind;

    fn test_mode() -> Mode {
        Mode {
            id: "message".into(),
            label: "Message".into(),
            preset: "message".into(),
            prompt: "Rewrite.".into(),
            stt: SttKind::Local,
            llm: LlmKind::OllamaLocal,
            language: "auto".into(),
            vocabulary: vec!["whisp".into()],
        }
    }

    #[test]
    fn prompt_contains_vocab_and_transcript() {
        let p = build_prompt("hello world", &test_mode());
        assert!(p.contains("whisp"));
        assert!(p.contains("hello world"));
    }

    #[test]
    fn passthrough_keeps_text() {
        let m = test_mode();
        let r = SelectedLlm::Passthrough.refine("  hi  ", &m).unwrap();
        assert_eq!(r.text, "hi");
    }
}
