//! LLM refinement for whisp.
//!
//! [`Llm::refine`] polishes a raw transcript according to the active mode.
//! - [`OllamaLocal`]: `whisp-nanbeige` (Nanbeige 4.2-3B, 100 % GPU) on `http://localhost:11434` (offline).
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
// Local: Ollama whisp-nanbeige (Nanbeige 4.2-3B, ctx 2048, 100 % GPU)
// ---------------------------------------------------------------------------

pub struct OllamaLocal {
    pub base_url: String,
    pub model: String,
    pub client: reqwest::blocking::Client,
}

/// Timeout total (connexion + lecture) des appels LLM. `connect_timeout`
/// séparé : sur DNS/réseau pendu, `timeout` seul peut attendre la connexion
/// TCP indéfiniment sur certaines plateformes ; 10 s de connect suffisent
/// (Ollama local répond en ms, cloud en ~100 ms).
const LLM_CONNECT_TIMEOUT_SECS: u64 = 10;

fn http_client() -> reqwest::blocking::Client {
    http_client_with_timeout(120)
}

fn http_client_with_timeout(secs: u64) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(secs))
        .connect_timeout(std::time::Duration::from_secs(LLM_CONNECT_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Pré-charge le modèle en VRAM (un token) : la première dictée ne paie
/// pas le reload (~7 s mesurés à froid). Best effort, silencieux.
pub fn warm() {
    let llm = OllamaLocal::default();
    let _ = llm
        .client
        .post(format!("{}/api/generate", llm.base_url))
        .json(&OllamaRequest {
            model: &llm.model,
            prompt: "ok",
            stream: false,
            think: false,
            keep_alive: "60m",
            // Même num_ctx que le raffinage (512) : Ollama clé le modèle
            // chargé sur le ctx — un warmup en ctx 32 forcerait un reload
            // à la première vraie requête (6 s mesurées).
            options: OllamaOptions {
                temperature: 0.0,
                num_ctx: 512,
                num_predict: 1,
            },
        })
        .send();
}

impl Default for OllamaLocal {
    fn default() -> Self {
        Self {
            // Config utilisateur (Settings → Modèles locaux), pas localhost en dur.
            base_url: crate::config::ollama_url(),
            model: crate::config::ollama_model(),
            // Nanbeige 4.2-3B met ~50 s pour 20 tokens sur CPU : marge large.
            client: http_client_with_timeout(300),
        }
    }
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_ctx: u32,
    num_predict: i32,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    think: bool,
    // Modèle gardé en VRAM 1 h : une dictée toutes les minutes ne paie
    // jamais le reload (~7 s mesurés). Défaut Ollama = 5 min.
    keep_alive: &'a str,
    options: OllamaOptions,
}

/// Coupe le bloc `<think>…</think>` si le modèle raisonne malgré `think: false`.
fn strip_think(s: &str) -> String {
    if let Some((_, after)) = s.split_once("</think>") {
        return after.trim().to_string();
    }
    if let Some((before, _)) = s.split_once("<think>") {
        return before.trim().to_string();
    }
    s.trim().to_string()
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
                // Pas de raisonnement pour du raffinage : direct + rapide.
                think: false,
                // num_ctx 512 : le prompt fait ~150 tokens, le contexte
                // 2048 alloue 4× de KV-cache pour rien (load + lent).
                // num_predict borné : le raffinage ne doit pas dépasser
                // ~1.5× le transcript (empêche le bavardage mesuré : 18 s
                // d'analyse+emojis pour une phrase).
                keep_alive: "60m",
                options: OllamaOptions {
                    temperature: 0.1,
                    num_ctx: 512,
                    num_predict: (transcript.split_whitespace().count() as i32 * 2 + 32).min(256),
                },
            })
            .send()
            .map_err(|e| {
                format!(
                    "ollama ({}) unreachable — is `ollama serve` running?: {e}",
                    self.base_url
                )
            })?
            .error_for_status()
            .map_err(|e| format!("ollama status: {e}"))?
            .json()
            .map_err(|e| format!("ollama decode: {e}"))?;
        Ok(Refined {
            text: strip_think(&resp.response),
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

/// Token OAuth abonnement Claude (`claude setup-token` → `sk-ant-oat-…`).
/// Stocké en fichier 600 `~/.config/whisp/claude_oauth.token` : le keyring
/// système est inutilisable sur ce host (cf. `fish_audio.key`).
fn claude_oauth_file() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join(".config")
        })
        .join("whisp/claude_oauth.token")
}

fn claude_oauth_token() -> Option<String> {
    // 1. Fichier 600 local (écrit par `set_claude_oauth_token`).
    if let Some(t) = std::fs::read_to_string(claude_oauth_file())
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        return Some(t);
    }
    // 2. Keyring (si l'host le supporte un jour).
    if let Some(t) = keyring_get("claude_oauth") {
        return Some(t);
    }
    // 3. Env (CI / setup-token exporté dans le shell).
    std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Style Claude (ton préréglé collé depuis claude.ai) : `system` injecté
/// dans les appels `claude` + `anthropic`. Vide = pas de bloc system.
fn claude_style() -> Option<String> {
    crate::config::load()
        .claude_style
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Enregistre le token OAuth abonnement (jamais loggué, jamais en config).
pub fn set_claude_oauth_token(token: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let token = token.trim();
    if token.is_empty() {
        return Err("empty token".to_string());
    }
    if !token.starts_with("sk-ant-oat-") && !token.starts_with("sk-ant-") {
        return Err(
            "token Claude invalide (attendu `sk-ant-oat-…` via `claude setup-token`)".to_string(),
        );
    }
    // Keyring best-effort (host actuel : échoue silencieusement).
    let _ = keyring::Entry::new("whisp", "claude_oauth")
        .and_then(|e| e.set_password(token).map_err(|_| keyring::Error::NoEntry));
    // Fichier 600 = source de vérité sur cet host.
    let path = claude_oauth_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("config dir: {e}"))?;
    }
    std::fs::write(&path, token).map_err(|e| format!("write oauth token: {e}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod oauth token: {e}"))?;
    Ok(())
}

/// Statut connexion abonnement : token présent oui/non (jamais le token).
pub fn claude_oauth_status() -> bool {
    claude_oauth_token().is_some()
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
            client: http_client(),
        }
    }
}

impl Llm for CloudAnthropic {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        if transcript.trim().is_empty() {
            return Ok(Refined {
                text: String::new(),
                engine: self.model.clone(),
            });
        }
        let key =
            keyring_get("anthropic").ok_or_else(|| "anthropic key not configured".to_string())?;
        let prompt = build_prompt(transcript, mode);
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];
        let text = anthropic_chat(&self.client, &key, &self.model, &msgs, true)?;
        Ok(Refined {
            text,
            engine: format!("anthropic:{}", self.model),
        })
    }
}

/// Raffinage dictée via abonnement Claude (OAuth + style). Utilisé quand un
/// mode demande `CloudAnthropic` et que l'OAuth est connecté sans clé API.
pub struct ClaudeSubscription {
    pub model: String,
    pub client: reqwest::blocking::Client,
}

impl Default for ClaudeSubscription {
    fn default() -> Self {
        Self {
            model: "claude-haiku-4-5".to_string(),
            client: http_client(),
        }
    }
}

impl Llm for ClaudeSubscription {
    fn refine(&self, transcript: &str, mode: &Mode) -> Result<Refined, String> {
        if transcript.trim().is_empty() {
            return Ok(Refined {
                text: String::new(),
                engine: self.model.clone(),
            });
        }
        let token = claude_oauth_token().ok_or_else(|| "compte Claude non connecté".to_string())?;
        let prompt = build_prompt(transcript, mode);
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];
        let text = claude_oauth_chat(&self.client, &token, &self.model, &msgs)?;
        Ok(Refined {
            text,
            engine: format!("claude:{}", self.model),
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
            client: http_client(),
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
/// `messages` are windowed to the last 50 turns (socket MCP peut envoyer
/// tout l'historique) ; the reply text is returned.
pub fn chat(provider: &str, messages: &[ChatMessage]) -> Result<String, String> {
    // Fenêtre bornée : seuls les 50 derniers tours partent au provider.
    // Raison : le socket MCP peut envoyer tout l'historique (100+ tours) —
    // payload JSON et facture explosent, et le modèle dilue le contexte.
    // 50 tours ≈ 4000 tokens, sous les limites de tous les providers.
    // `saturating_sub` : pas de underflow quand < 50 messages.
    const MAX_CHAT_TURNS: usize = 50;
    let start = messages.len().saturating_sub(MAX_CHAT_TURNS);
    let messages = &messages[start..];
    let client = http_client();
    match provider {
        "anthropic" => {
            let key = keyring_get("anthropic")
                .ok_or_else(|| "anthropic key not configured".to_string())?;
            anthropic_chat(&client, &key, "claude-haiku-4-5", messages, false)
        }
        // Abonnement Claude (OAuth `sk-ant-oat-…`, pas de clé API) :
        // `Authorization: Bearer` + beta `oauth-2025-04-20`, style injecté.
        "claude" => {
            let token = claude_oauth_token()
                .ok_or_else(|| "compte Claude non connecté — collez le token `claude setup-token` dans Settings → Claude".to_string())?;
            let cfg = crate::config::load();
            let model = if cfg.chat_provider.as_deref() == Some("claude") {
                cfg.chat_model
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "claude-haiku-4-5".to_string())
            } else {
                "claude-haiku-4-5".to_string()
            };
            claude_oauth_chat(&client, &token, &model, messages)
        }
        "openai" => openai_chat(
            &client,
            "https://api.openai.com/v1",
            "openai",
            "gpt-4o-mini",
            messages,
        ),
        "google" | "openrouter" | "deepseek" | "qwen" | "moonshot" | "custom" => {
            // OpenAI-compatible : base URL + modèle depuis la config, clé keyring du provider.
            let (base, fallback) = match provider {
                "google" => (
                    "https://generativelanguage.googleapis.com/v1beta/openai",
                    "gemini-2.0-flash",
                ),
                "openrouter" => ("https://openrouter.ai/api/v1", "openrouter/auto"),
                "deepseek" => ("https://api.deepseek.com/v1", "deepseek-chat"),
                "qwen" => (
                    "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
                    "qwen-plus",
                ),
                "moonshot" => ("https://api.moonshot.ai/v1", "moonshot-v1-8k"),
                _ => ("https://api.openai.com/v1", "gpt-4o-mini"),
            };
            let cfg = crate::config::load();
            let base = cfg
                .chat_base_url
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| base.to_string());
            // Modèle : config si le provider actif correspond, sinon fallback.
            let active_is_provider = cfg.chat_provider.as_deref() == Some(provider);
            let model = if active_is_provider {
                cfg.chat_model
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| fallback.to_string())
            } else {
                fallback.to_string()
            };
            openai_chat(&client, &base, provider, &model, messages)
        }
        _ => {
            // Default: local Ollama `/api/chat` (offline, no key).
            let url = format!(
                "{}/api/chat",
                crate::config::ollama_url().trim_end_matches('/')
            );
            let turns: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
                .collect();
            if turns.is_empty() {
                return Err("empty conversation".to_string());
            }
            let resp: serde_json::Value = client
                .post(url)
                .json(&serde_json::json!({
                    "model": crate::config::ollama_model(),
                    "messages": turns,
                    "stream": false,
                    "think": false,
                    // Même keep_alive que le raffinage : le mode voix ne
                    // paie pas le reload non plus. ctx 512 : conversation.
                    "keep_alive": "60m",
                    "options": { "num_ctx": 512 },
                }))
                .send()
                .map_err(|e| format!("ollama unreachable — is `ollama serve` running?: {e}"))?
                .error_for_status()
                .map_err(|e| format!("ollama status: {e}"))?
                .json()
                .map_err(|e| format!("ollama decode: {e}"))?;
            Ok(strip_think(
                resp.pointer("/message/content")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default(),
            ))
        }
    }
}

/// Chat OpenAI-compatible générique (OpenAI, Gemini/DeepSeek/Qwen/Moonshot via base URL, custom).
fn openai_chat(
    client: &reqwest::blocking::Client,
    base_url: &str,
    provider: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let key = keyring_get(provider).ok_or_else(|| format!("{provider} key not configured"))?;
    let turns: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
        .collect();
    if turns.is_empty() {
        return Err("empty conversation".to_string());
    }
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let resp: serde_json::Value = client
        .post(url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "model": model, "messages": turns }))
        .send()
        .map_err(|e| format!("{provider} request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("{provider} status: {e}"))?
        .json()
        .map_err(|e| format!("{provider} decode: {e}"))?;
    Ok(resp
        .pointer("/choices/0/message/content")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// Corps `/v1/messages` partagé : turns + `system` (style Claude + systèmes
/// éventuels). Le style est ajouté en premier bloc, jamais loggué.
fn messages_body(
    model: &str,
    messages: &[ChatMessage],
    with_style: bool,
) -> Result<serde_json::Value, String> {
    // `claude_style()` lit la config à chaque appel : pas de cache périmé.
    // Le String doit survivre à `system` → propriétaire `style_owned`.
    let style_owned: Option<String> = if with_style { claude_style() } else { None };
    let mut system: Vec<&str> = Vec::new();
    if let Some(s) = style_owned.as_deref() {
        system.push(s);
    }
    for m in messages.iter().filter(|m| m.role == "system") {
        system.push(m.content.as_str());
    }
    let turns: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| serde_json::json!({ "role": chat_role(&m.role), "content": m.content }))
        .collect();
    if turns.is_empty() {
        return Err("empty conversation".to_string());
    }
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": turns,
    });
    if !system.is_empty() {
        body["system"] = serde_json::Value::String(system.join("\n"));
    }
    Ok(body)
}

fn parse_messages_text(resp: &serde_json::Value) -> String {
    resp.get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Chat via clé API Anthropic (`x-api-key`), style Claude injecté.
fn anthropic_chat(
    client: &reqwest::blocking::Client,
    key: &str,
    model: &str,
    messages: &[ChatMessage],
    with_style: bool,
) -> Result<String, String> {
    let body = messages_body(model, messages, with_style)?;
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
    Ok(parse_messages_text(&resp))
}

/// Chat via abonnement Claude (`Authorization: Bearer sk-ant-oat-…` +
/// `anthropic-beta: oauth-2025-04-20`), style Claude injecté.
fn claude_oauth_chat(
    client: &reqwest::blocking::Client,
    token: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let body = messages_body(model, messages, true)?;
    let resp: serde_json::Value = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20")
        .json(&body)
        .send()
        .map_err(|e| format!("claude request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("claude status: {e}"))?
        .json()
        .map_err(|e| format!("claude decode: {e}"))?;
    Ok(parse_messages_text(&resp))
}

 /// Tauri command: blocking chat completion for the Chat tab.
 #[tauri::command]
pub async fn chat_command(messages: Vec<ChatMessage>, provider: String) -> Result<String, String> {
    // `chat()` = reqwest::blocking jusqu'à 120 s : JAMAIS sur le thread
    // principal (freeze total constaté). spawn_blocking = thread pool dédié.
    // (reqwest::blocking paniquerait dans un contexte async direct.)
    tauri::async_runtime::spawn_blocking(move || chat(&provider, &messages))
        .await
        .map_err(|e| format!("chat worker: {e}"))?
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
    Claude(ClaudeSubscription),
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
            SelectedLlm::Claude(l) => l.refine(transcript, mode),
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
            } else if claude_oauth_token().is_some() {
                // Abonnement connecté sans clé API : raffinage via OAuth + style.
                SelectedLlm::Claude(ClaudeSubscription::default())
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
