import { writable } from "svelte/store";
import { fetchAppConfig } from "./tauri";

export { migrateStoredOllamaModel } from "./chatProviders";

export type PopupPos = "top-center" | "center" | "bottom-center" | "cursor";
export type SttModel = "base" | "small" | "medium";
export type Theme = "system" | "dark" | "light";
export type Language = "fr" | "en" | "auto";
export type ChatProvider = "claude" | "anthropic" | "openai" | string;

export function isPopupPos(v: unknown): v is PopupPos {
  return v === "top-center" || v === "center" || v === "bottom-center" || v === "cursor";
}

function isSttModel(v: unknown): v is SttModel {
  return v === "base" || v === "small" || v === "medium";
}

function isTheme(v: unknown): v is Theme {
  return v === "system" || v === "dark" || v === "light";
}

function isLanguage(v: unknown): v is Language {
  return v === "fr" || v === "en" || v === "auto";
}

function asString(v: unknown): string | undefined {
  return typeof v === "string" && v.trim() !== "" ? v : undefined;
}

function asBool(v: unknown): boolean | undefined {
  return typeof v === "boolean" ? v : undefined;
}

function asStringArray(v: unknown): string[] | undefined {
  if (!Array.isArray(v)) return undefined;
  const kept = (v as unknown[]).filter((x): x is string => typeof x === "string");
  return kept.length > 0 ? kept : undefined;
}

export interface BackendConfig {
  popupPos?: PopupPos;
  sttModel?: SttModel;
  theme?: Theme;
  accentSource?: string;
  accentCustom?: string;
  language?: Language;
  inputDevice?: string;
  agent?: string;
  agentCommand?: string;
  autoPaste?: boolean;
  realtimePreview?: boolean;
  activePills?: string[];
  ollamaUrl?: string;
  ollamaModel?: string;
  chatProvider?: ChatProvider;
  chatModel?: string;
  chatBaseUrl?: string;
  claudeStyle?: string;
}

/** Raw backend (serde camelCase, champs optionnels) -> typé strict avec garde-fous.
 *  Jamais de throw : un champ invalide est ignoré, le défaut backend s'applique. */
export function parseBackendConfig(raw: unknown): BackendConfig {
  if (!raw || typeof raw !== "object") return {};
  const r = raw as Record<string, unknown>;
  const out: BackendConfig = {};
  if (isPopupPos(r.popupPos)) out.popupPos = r.popupPos;
  if (isSttModel(r.sttModel)) out.sttModel = r.sttModel;
  if (isTheme(r.theme)) out.theme = r.theme;
  if (isLanguage(r.language)) out.language = r.language;
  const s = (k: string) => asString(r[k]);
  const b = (k: string) => asBool(r[k]);
  const arr = (k: string) => asStringArray(r[k]);
  if (s("accentSource")) out.accentSource = s("accentSource");
  if (s("accentCustom")) out.accentCustom = s("accentCustom");
  if (s("inputDevice") !== undefined) out.inputDevice = r.inputDevice as string;
  if (s("agent")) out.agent = s("agent");
  if (s("agentCommand")) out.agentCommand = s("agentCommand");
  if (b("autoPaste") !== undefined) out.autoPaste = b("autoPaste");
  if (b("realtimePreview") !== undefined) out.realtimePreview = b("realtimePreview");
  if (arr("activePills")) out.activePills = migrateActivePills(r.activePills) ?? arr("activePills");
  if (s("ollamaUrl")) out.ollamaUrl = s("ollamaUrl");
  if (s("ollamaModel")) out.ollamaModel = s("ollamaModel");
  if (s("chatProvider")) out.chatProvider = s("chatProvider");
  if (s("chatModel")) out.chatModel = s("chatModel");
  if (s("chatBaseUrl")) out.chatBaseUrl = s("chatBaseUrl");
  if (typeof r.claudeStyle === "string") out.claudeStyle = r.claudeStyle;
  return out;
}

/** Migration `sta` → `agent` (ancien nom de pill) : backend source de vérité. */
export function migrateActivePills(pills: unknown): string[] | null {
  if (!Array.isArray(pills)) return null;
  const kept = (pills as unknown[])
    .filter((p): p is string => typeof p === "string")
    .map((p) => (p === "sta" ? "agent" : p));
  return kept.length > 0 ? kept : null;
}

/** Config backend partagée (Settings ↔ Recorder) : un seul `get_config` au mount. */
export const backendConfig = writable<BackendConfig>({});

export async function refreshBackendConfig(): Promise<BackendConfig> {
  const raw = await fetchAppConfig();
  const cfg = parseBackendConfig(raw);
  backendConfig.set(cfg);
  return cfg;
}
