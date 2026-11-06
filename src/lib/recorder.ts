import { invoke } from "@tauri-apps/api/core";
import { currentMonitor, getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { fetchAppConfig } from "./appConfig";
import { readPref } from "./prefs";

export type RecorderStatus = "idle" | "recording" | "processing" | "done" | "error" | "speaking";

export type PopupPosition = "top-center" | "center" | "bottom-center" | "cursor";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function safeInvoke(cmd: string): Promise<void> {
  try {
    await invoke(cmd);
  } catch {
    /* browser dev without backend */
  }
}

export { safeInvoke };

/** Sauve la position logique en localStorage + backend. */
export function savePopupXY(x: number, y: number): void {
  const v = `${Math.round(x)},${Math.round(y)}`;
  try {
    localStorage.setItem("whisp:popup-xy", v);
  } catch {
    /* stockage indisponible */
  }
  void invoke("set_config", { popupXy: v }).catch(() => {
    /* backend en cours : localStorage seul */
  });
}

/** Opacité carte (Apparence) : CSS var lue en plain string. */
export function applyPopupAppearance(): void {
  const raw = Number(readPref("whisp:card-opacity"));
  const alpha = Number.isFinite(raw) && raw >= 0.4 && raw <= 1 ? raw : 1;
  document.documentElement.style.setProperty("--card-alpha", String(alpha));
}

/** Accent d'une config backend (`get_config`, camelCase) : backend d'abord,
 * localStorage en repli (navigateur/hors-ligne). Même résolution que Settings
 * (source → custom → scheme), jamais de localStorage isolé comme vérité. */
export function resolveAppearanceFromConfig(cfg: Record<string, unknown> | null | undefined): {
  theme: string | null;
  source: "theme" | "custom" | "caelestia";
  custom: string;
} {
  const theme =
    cfg?.theme === "dark" || cfg?.theme === "light" ? (cfg.theme as string) : null;
  const source =
    cfg?.accentSource === "custom" || cfg?.accentSource === "caelestia" ? (cfg.accentSource as "custom" | "caelestia") : "theme";
  const rawCustom = typeof cfg?.accentCustom === "string" ? (cfg.accentCustom as string) : "";
  const custom =
    rawCustom.length === 7 && rawCustom.startsWith("#") && [...rawCustom.slice(1)].every((ch) => /[0-9a-fA-F]/.test(ch))
      ? rawCustom.toLowerCase()
      : (readPref("whisp:accent-custom") ?? "#f59e0b");
  return { theme, source, custom };
}

/** Position initiale : XY mémorisé d'abord, sinon preset + backend. */
export async function applyPopupPosition(): Promise<void> {
  if (!isTauri()) return;
  try {
    const saved = readPref("whisp:popup-xy");
    if (saved) {
      const [sx, sy] = saved.split(",").map(Number);
      if (Number.isFinite(sx) && Number.isFinite(sy) && (sx !== 0 || sy !== 0)) {
        await getCurrentWindow().setPosition(new LogicalPosition(Math.round(sx), Math.round(sy)));
        return;
      }
    }
  } catch {
    /* pas de XY sauvé : calcul classique */
  }
  let pos: PopupPosition = "top-center";
  const stored = readPref("whisp:popup-pos");
  if (stored === "center" || stored === "bottom-center" || stored === "cursor") pos = stored;
  // get_config partagé (single-flight) : un seul appel avec Settings.
  const cfg = await fetchAppConfig();
  if (
    cfg?.popupPos === "top-center" ||
    cfg?.popupPos === "center" ||
    cfg?.popupPos === "bottom-center" ||
    cfg?.popupPos === "cursor"
  ) {
    pos = cfg.popupPos as PopupPosition;
  }
  try {
    const win = getCurrentWindow();
    const monitor = await currentMonitor();
    const size = await win.innerSize();
    const factor = monitor?.scaleFactor ?? 1;
    const toLogical = (px: number): number => px / factor;
    if (pos === "center" || pos === "cursor") {
      await win.center().catch(() => undefined);
      return;
    }
    if (!monitor) {
      await win.center().catch(() => undefined);
      return;
    }
    const monW = toLogical(monitor.size.width);
    const monX = toLogical(monitor.position.x);
    const winW = toLogical(size.width);
    const winH = toLogical(size.height);
    const x = monX + Math.max(0, (monW - winW) / 2);
    if (pos === "top-center") {
      const y = toLogical(monitor.position.y) + 48;
      await win.setPosition(new LogicalPosition(Math.round(x), Math.round(y)));
    } else {
      const monH = toLogical(monitor.size.height);
      const monY = toLogical(monitor.position.y);
      const y = monY + Math.max(0, monH - winH - 96);
      await win.setPosition(new LogicalPosition(Math.round(x), Math.round(y)));
    }
  } catch {
    /* positioning best-effort */
  }
}

export function toStatus(raw: unknown): RecorderStatus {
  if (raw === "idle" || raw === "recording" || raw === "processing" || raw === "done" || raw === "error") {
    return raw;
  }
  return "recording";
}

export function parseState(payload: unknown): { status: RecorderStatus; message?: string; mode?: string } {
  if (typeof payload === "string") return { status: toStatus(payload) };
  if (payload && typeof payload === "object") {
    const o = payload as Record<string, unknown>;
    const out: { status: RecorderStatus; message?: string; mode?: string } = {
      status: toStatus(o.status),
    };
    if (typeof o.message === "string") out.message = o.message;
    if (typeof o.mode === "string") out.mode = o.mode;
    return out;
  }
  return { status: "recording" };
}

export function parseLevel(payload: unknown): number {
  if (typeof payload === "number" && Number.isFinite(payload)) {
    return Math.max(0, Math.min(1, payload));
  }
  if (payload && typeof payload === "object") {
    const v = (payload as Record<string, unknown>).level;
    if (typeof v === "number" && Number.isFinite(v)) return Math.max(0, Math.min(1, v));
  }
  return 0;
}

export interface Approval {
  request_id: string;
  session_id: string;
  agent: string;
  tool: string;
  command: string;
}

/** Largeurs popup S/M/L -20 %. Hauteur suivie par l'autosize. */
export function popupWidth(mini: boolean): number {
  const cfgSize = readPref("whisp:popup-size");
  const base = cfgSize === "S" ? 512 : cfgSize === "L" ? 704 : 576;
  return mini ? Math.min(336, base) : base;
}

export const MIC_ICON =
  `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">` +
  `<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />` +
  `<path d="M19 10v2a7 7 0 0 1-14 0v-2" />` +
  `<line x1="12" x2="12" y1="19" y2="22" />` +
  `</svg>`;

export const SHRINK_ICON =
  `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">` +
  `<path d="m14 10 7-7" />` +
  `<path d="M20 10V4h-6" />` +
  `<path d="m10 14-7 7" />` +
  `<path d="M4 14v6h6" />` +
  `</svg>`;

export const EXPAND_ICON =
  `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">` +
  `<path d="M15 3h6v6" />` +
  `<path d="m21 3-7 7" />` +
  `<path d="m9 21-7-7" />` +
  `<path d="M3 15v6h6" />` +
  `</svg>`;
