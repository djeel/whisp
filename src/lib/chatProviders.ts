/** Source unique des providers chat (Chat + Settings). `custom` inclus ici
 * pour que l'onglet Chat ne retombe jamais sur ollama en silence. */

export type ChatProviderId =
  | "ollama"
  | "claude"
  | "anthropic"
  | "openai"
  | "google"
  | "openrouter"
  | "deepseek"
  | "qwen"
  | "moonshot"
  | "custom";

export const CHAT_PROVIDERS: Array<{ id: ChatProviderId; label: string }> = [
  { id: "ollama", label: "Ollama (local)" },
  { id: "claude", label: "Claude (abonnement)" },
  { id: "anthropic", label: "Anthropic (clé API)" },
  { id: "openai", label: "OpenAI" },
  { id: "google", label: "Gemini (OpenAI-compatible)" },
  { id: "openrouter", label: "OpenRouter" },
  { id: "deepseek", label: "DeepSeek" },
  { id: "qwen", label: "Qwen" },
  { id: "moonshot", label: "Moonshot" },
  { id: "custom", label: "Serveur OpenAI-compatible" },
];

export function isChatProvider(v: unknown): v is ChatProviderId {
  return CHAT_PROVIDERS.some((p) => p.id === v);
}

/** Config par provider : modèle + baseUrl suivent le provider, pas l'inverse.
 * Stocké sous `whisp:chat-per-provider` : { [provider]: { model, baseUrl } }. */

export interface ProviderConf {
  model: string;
  baseUrl: string;
}

const KEY = "whisp:chat-per-provider";

export function loadProviderConfs(): Record<string, ProviderConf> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const v = JSON.parse(raw) as unknown;
    return typeof v === "object" && v !== null ? (v as Record<string, ProviderConf>) : {};
  } catch {
    return {};
  }
}

export function saveProviderConfs(confs: Record<string, ProviderConf>): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(confs));
  } catch {
    /* storage unavailable */
  }
}

const OLLAMA_LEGACY_DEFAULTS = ["qwen2.5:3b", "ndavat/Nanbeige4.2-3B"];
export const OLLAMA_DEFAULT_MODEL = "whisp-nanbeige";
const PROVIDER_DEFAULTS: Record<string, ProviderConf> = {
  ollama: { model: OLLAMA_DEFAULT_MODEL, baseUrl: "" },
  claude: { model: "claude-haiku-4-5", baseUrl: "" },
  anthropic: { model: "claude-haiku-4-5", baseUrl: "" },
  openai: { model: "gpt-4o-mini", baseUrl: "" },
  google: { model: "gemini-2.0-flash", baseUrl: "" },
  openrouter: { model: "openrouter/auto", baseUrl: "" },
  deepseek: { model: "deepseek-chat", baseUrl: "" },
  qwen: { model: "qwen-plus", baseUrl: "" },
  moonshot: { model: "moonshot-v1-8k", baseUrl: "" },
  custom: { model: "", baseUrl: "" },
};

export function getProviderConf(confs: Record<string, ProviderConf>, provider: string): ProviderConf {
  return confs[provider] ?? PROVIDER_DEFAULTS[provider] ?? { model: "", baseUrl: "" };
}

/** Migre l'ancien défaut stocké vers Nanbeige (variante locale 100 % GPU).
 * Les choix explicites de l'utilisateur sont conservés tels quels. */
export function migrateStoredOllamaModel(stored: string | null): string {
  if (!stored || OLLAMA_LEGACY_DEFAULTS.includes(stored)) return OLLAMA_DEFAULT_MODEL;
  return stored;
}
