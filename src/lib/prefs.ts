/** Shared preference codec: one encoding for localStorage (no more
 * `"dark"`-with-quotes vs `dark` mismatch between App and Recorder). */

import { DARK, LIGHT, themeVars } from "./tokens";

export function readPref(key: string): string | null {
  try {
    const raw = localStorage.getItem(key);
    if (raw == null) return null;
    try {
      const v = JSON.parse(raw) as unknown;
      return typeof v === "string" ? v : raw;
    } catch {
      return raw;
    }
  } catch {
    return null;
  }
}

export function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* storage unavailable */
  }
}

/** Resolve "system" via matchMedia, always return dark|light. */
export function resolveTheme(pref: string | null): "dark" | "light" {
  if (pref === "dark") return "dark";
  if (pref === "light") return "light";
  try {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  } catch {
    return "dark";
  }
}

/** Apply resolved theme to <html data-theme="dark|light"> + tokens vars.
 * Les variables viennent de `tokens.ts` (source de vérité façon Herdr
 * `themes/*.json`) : App.css ne fait que les consommer. */
export function applyTheme(pref: string | null): "dark" | "light" {
  const resolved = resolveTheme(pref);
  try {
    const root = document.documentElement;
    root.dataset.theme = resolved;
    const vars = themeVars(resolved === "light" ? LIGHT : DARK);
    for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  } catch {
    /* no DOM */
  }
  return resolved;
}

/** JSON helpers (tableaux/objets). */
export function readJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function writeJSON(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable */
  }
}
