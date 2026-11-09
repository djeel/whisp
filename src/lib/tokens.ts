/** Design tokens whisp — source de vérité, façon Herdr `themes/*.json`.
 *
 * Herdr garde couleurs/layouts/mods en fichiers (`~/.config/herdr-app`) :
 * ici les tokens vivent en TS (typé, importable) et App.css ne fait que les
 * appliquer en variables CSS. Un seul endroit pour radius, espacements,
 * ombres, status — fini les `10px/12px/16px` dispersés dans 1200 lignes CSS.
 *
 * Convention : `surface` > `raised` > `overlay` (comme Herdr `background` /
 * `surface` / `popover` / `raised`). Tout composant utilise ces clés, jamais
 * de hex en dur. */

export interface WhispTheme {
  background: string;
  surface: string;
  raised: string;
  input: string;
  foreground: string;
  muted: string;
  dim: string;
  border: string;
  hover: string;
  accent: string;
  accentForeground: string;
  ring: string;
  working: string;
  blocked: string;
  done: string;
  idle: string;
}

/** Thème sombre (défaut) — garde la palette whisp actuelle. */
export const DARK: WhispTheme = {
  background: "#141414",
  surface: "#1e1e1e",
  raised: "#262626",
  input: "#141414",
  foreground: "#e8e8e8",
  muted: "#9a9a9a",
  dim: "#6b6b6b",
  border: "rgba(255, 255, 255, 0.06)",
  hover: "rgba(255, 255, 255, 0.06)",
  accent: "#e8e8e8",
  accentForeground: "#141414",
  ring: "#e8e8e8",
  working: "#4ade80",
  blocked: "#ef4444",
  done: "#60a5fa",
  idle: "#9a9a9a",
};

/** Thème clair — mêmes clés, mêmes contrastes AA. */
export const LIGHT: WhispTheme = {
  background: "#efefef",
  surface: "#f5f5f5",
  raised: "#ffffff",
  input: "#ffffff",
  foreground: "#1a1a1a",
  muted: "#595959",
  dim: "#8a8a8a",
  border: "rgba(0, 0, 0, 0.08)",
  hover: "rgba(0, 0, 0, 0.05)",
  accent: "#1a1a1a",
  accentForeground: "#f5f5f5",
  ring: "#1a1a1a",
  working: "#16a34a",
  blocked: "#dc2626",
  done: "#2563eb",
  idle: "#6b6b6b",
};

/** Échelle unique — tout rayon/espace vient d'ici. */
export const RADIUS = {
  sm: 8,
  md: 10,
  panel: 16,
  card: 24,
  pill: 999,
} as const;

export const SPACE = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  xxl: 24,
} as const;

/** Ombres : carte recorder (flottante) vs popup (menu). */
export const SHADOW = {
  card: "0 18px 50px rgba(0, 0, 0, 0.45)",
  popup: "0 12px 32px rgba(0, 0, 0, 0.5)",
  none: "none",
} as const;

/** Écrit le thème en variables CSS sur `:root` (appelée par applyTheme). */
export function themeVars(t: WhispTheme): Record<string, string> {
  return {
    "--bg-app": t.background,
    "--bg-card": t.surface,
    "--bg-bar": t.background,
    "--bg-input": t.input,
    "--text": t.foreground,
    "--muted": t.muted,
    "--chip": t.raised,
    "--line": t.border,
    "--hover-wash": t.hover,
    "--accent": t.accent,
    "--accent-text": t.accentForeground,
    "--green": t.working,
    "--red": t.blocked,
    "--wave-idle": t.idle,
    "--wave-active": t.foreground,
    "--radius-card": `${RADIUS.card}px`,
    "--radius-panel": `${RADIUS.panel}px`,
  };
}
