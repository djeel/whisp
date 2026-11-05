import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy } from "svelte";

/** Hors Tauri (navigateur/dev) : pas d'IPC, repli localStorage seul. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** invoke typé : même signature que `@tauri-apps/api/core`. */
export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/** listen typé : même signature que `@tauri-apps/api/event`. */
export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return tauriListen<T>(event, handler);
}

/** Écoute un event backend, désabonnée à la destruction du composant.
 * Retourne `unlisten` pour un nettoyage manuel (hors composant). */
export function onEvent<T>(event: string, handler: EventCallback<T>): () => void {
  let off: UnlistenFn | null = null;
  let live = true;
  void tauriListen<T>(event, handler)
    .then((un) => {
      if (!live) {
        un();
        return;
      }
      off = un;
    })
    .catch(() => {
      /* navigateur : pas d'événements backend */
    });
  const unlisten = (): void => {
    live = false;
    if (off) {
      off();
      off = null;
    }
  };
  try {
    onDestroy(unlisten);
  } catch {
    /* hors composant : l'appelant garde unlisten */
  }
  return unlisten;
}

/** Config backend partagée : single-flight (un seul `get_config` en vol,
 * tous les consommateurs du mount le réutilisent). Lecture fichier <1 ms
 * mais 3 appels parallèles au mount = 3 parses + 3 états à réconcilier. */
let inflight: Promise<Record<string, unknown>> | null = null;

export function fetchAppConfig(): Promise<Record<string, unknown>> {
  if (inflight) return inflight;
  if (!isTauri()) {
    inflight = Promise.resolve({});
    return inflight;
  }
  inflight = tauriInvoke<Record<string, unknown>>("get_config")
    .then((cfg) => (cfg && typeof cfg === "object" ? cfg : {}))
    .catch(() => ({} as Record<string, unknown>))
    .finally(() => {
      // Fenêtre suivante refetch (HMR / retour onglet) : pas de cache périmé.
      inflight = null;
    });
  return inflight;
}

// Debounce IPC : un seul set_config 400 ms après la dernière frappe.
// Avant : chaque onChange (chatModel, URLs, agentCommand, slider opacité)
// = un invoke → backend spammé + race sur config.json.
let configTimer: number | null = null;
let configPending: Record<string, unknown> = {};

export function queueConfig(patch: Record<string, unknown>): void {
  Object.assign(configPending, patch);
  if (configTimer !== null) window.clearTimeout(configTimer);
  configTimer = window.setTimeout(() => {
    void flushQueuedConfig();
  }, 400);
}

export function flushQueuedConfig(): Promise<void> {
  const payload = configPending;
  configPending = {};
  configTimer = null;
  if (Object.keys(payload).length === 0) return Promise.resolve();
  return tauriInvoke("set_config", payload)
    .catch(() => {
      /* backend en cours : localStorage seul */
    })
    .then(() => undefined);
}

/** À appeler pendant l'init d'un composant : flush du debounce à la destruction. */
export function useConfigFlushOnDestroy(): void {
  onDestroy(() => {
    if (configTimer !== null) {
      window.clearTimeout(configTimer);
      configTimer = null;
    }
    void flushQueuedConfig();
  });
}
