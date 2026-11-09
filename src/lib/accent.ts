import { invoke, isTauri } from "./tauri";
import { applyTheme, readPref, writePref } from "./prefs";
import { writable } from "svelte/store";

/** Accent du recorder : `theme` (défaut, suit clair/sombre) |
 * `custom` (couleur choisie) | `caelestia` (scheme Caelestia, suit le fond d'écran). */
export type AccentSource = "theme" | "custom" | "caelestia";

export interface CaelestiaScheme {
  available: boolean;
  mode: string;
  primary: string;
  onPrimary: string;
  surface: string;
  onSurface: string;
  background: string;
}

export const DEFAULT_ACCENT = "#f59e0b";

export function isAccentHex(v: unknown): v is string {
  return (
    typeof v === "string" &&
    v.length === 7 &&
    v.startsWith("#") &&
    [...v.slice(1)].every((ch) => /[0-9a-fA-F]/.test(ch))
  );
}

export function parseAccentSource(v: unknown): AccentSource | null {
  return v === "theme" || v === "custom" || v === "caelestia" ? v : null;
}

export function readAccentSource(): AccentSource {
  return parseAccentSource(readPref("whisp:accent-source")) ?? "theme";
}

export function readAccentCustom(): string {
  const raw = readPref("whisp:accent-custom");
  return isAccentHex(raw) ? raw : DEFAULT_ACCENT;
}

/** Texte lisible sur un accent : sombre si clair, clair si sombre. */
export function textOnAccent(hex: string): string {
  const n = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const lin = n.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  const lum = 0.2126 * (lin[0] ?? 0) + 0.7152 * (lin[1] ?? 0) + 0.0722 * (lin[2] ?? 0);
  return lum > 0.4 ? "#141414" : "#f5f5f5";
}

/** Variables posées par l'accent : SEULEMENT l'accent, jamais le fond.
 * L'ancien `ACCENT_VARS` incluait `--bg-card/--bg-app/--text` : Caelestia
 * réécrivait tout le thème et le mode light ne s'appliquait plus. Le scheme
 * Caelestia ne pose plus que `--accent*` + `--wave-active`, le thème reste
 * piloté par tokens.ts via `data-theme`. */
const ACCENT_VARS = ["--accent", "--accent-text", "--wave-active"] as const;

export function clearAccentOverrides(): void {
  try {
    const st = document.documentElement.style;
    for (const v of ACCENT_VARS) st.removeProperty(v);
  } catch {
    /* no DOM */
  }
}

/**
 * Applique l'accent (bouton Stop + waveform active + détails, via `--accent`
 * partagée app/recorder). `custom`/`caelestia` : remplace le thème ; `theme` :
 * retire les surcharges. Émet `whisp:theme` pour la palette Waveform en cache.
 * Repli thème quand le scheme Caelestia est absent.
 */
export function applyAccent(source: AccentSource, custom: string, scheme: CaelestiaScheme | null): void {
  clearAccentOverrides();
  try {
    const st = document.documentElement.style;
    if (source === "custom" && isAccentHex(custom)) {
      st.setProperty("--accent", custom.toLowerCase());
      st.setProperty("--accent-text", textOnAccent(custom));
      st.setProperty("--wave-active", custom.toLowerCase());
    } else if (source === "caelestia" && scheme?.available) {
      // Le scheme ne pilote que l'accent : le fond/texte reste au thème
      // (tokens.ts). Évite que le fond d'écran casse le mode light.
      st.setProperty("--accent", scheme.primary);
      st.setProperty("--accent-text", scheme.onPrimary);
      st.setProperty("--wave-active", scheme.primary);
    }
  } catch {
    /* no DOM */
  }
  try {
    window.dispatchEvent(new Event("whisp:theme"));
  } catch {
    /* no DOM */
  }
}

 /** Thème + accent d'un coup (ordre : thème d'abord, accent par-dessus). */
 export function applyAppearance(themePref: string | null, source: AccentSource, custom: string, scheme: CaelestiaScheme | null): void {
  applyTheme(themePref);
  applyAccent(source, custom, scheme);
}

/** Scheme Caelestia via backend ; null hors Tauri ou Caelestia absent. */
export async function fetchCaelestiaScheme(): Promise<CaelestiaScheme | null> {
  if (!isTauri()) return null;
  try {
    const s = await invoke<CaelestiaScheme>("caelestia_scheme");
    return s && s.available ? s : null;
  } catch {
    return null;
  }
}

/** Démarre le suivi du fond d'écran (backend, idempotent). */
export function startCaelestiaWatch(): void {
  if (!isTauri()) return;
  void invoke("caelestia_watch").catch(() => {
    /* backend en cours */
  });
}

export function persistAccent(source: AccentSource, custom: string): void {
  writePref("whisp:accent-source", source);
  if (isAccentHex(custom)) writePref("whisp:accent-custom", custom.toLowerCase());
}

/** Stores partagés (Settings ↔ Recorder) : mêmes clés `whisp:accent-*`. */

export const accentSource = writable<AccentSource>(readAccentSource());

export const accentCustom = writable<string>(readAccentCustom());

export const caelestia = writable<CaelestiaScheme | null>(null);

/** Alias historique : même store que `caelestia`. */
export const scheme = caelestia;
