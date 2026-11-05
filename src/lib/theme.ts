import { writable } from "svelte/store";
import { applyTheme, readPref, writePref } from "./prefs";

export type ThemePref = "system" | "dark" | "light";

export function parseThemePref(v: unknown): ThemePref {
  return v === "dark" || v === "light" ? v : "system";
}

export { applyTheme };

/** Thème partagé (Settings ↔ Recorder) : même clé `whisp:theme`. */
export const theme = writable<ThemePref>(parseThemePref(readPref("whisp:theme")));

/** Applique + persiste (même ordre que `applyAppearance` : thème d'abord). */
export function setThemePref(next: ThemePref): void {
  theme.set(next);
  writePref("whisp:theme", next);
  applyTheme(next === "system" ? null : next);
}

/** Mount : applique le thème stocké (`system` → matchMedia via `resolveTheme`). */
export function initTheme(): "dark" | "light" {
  const pref = parseThemePref(readPref("whisp:theme"));
  return applyTheme(pref === "system" ? null : pref);
}
