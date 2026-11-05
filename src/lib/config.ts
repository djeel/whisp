import { writable } from "svelte/store";
import { fetchAppConfig } from "./tauri";

export { migrateStoredOllamaModel } from "./chatProviders";

export type PopupPos = "top-center" | "center" | "bottom-center" | "cursor";

export function isPopupPos(v: unknown): v is PopupPos {
  return v === "top-center" || v === "center" || v === "bottom-center" || v === "cursor";
}

export interface BackendConfig {
  popupPos?: unknown;
  sttModel?: unknown;
  theme?: unknown;
  accentSource?: unknown;
  accentCustom?: unknown;
  language?: unknown;
  inputDevice?: unknown;
  agent?: unknown;
  agentCommand?: unknown;
  autoPaste?: unknown;
  realtimePreview?: unknown;
  activePills?: unknown;
  ollamaUrl?: unknown;
  ollamaModel?: unknown;
  chatProvider?: unknown;
  chatModel?: unknown;
  chatBaseUrl?: unknown;
  claudeStyle?: unknown;
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
  const cfg = (await fetchAppConfig()) as BackendConfig;
  backendConfig.set(cfg);
  return cfg;
}
