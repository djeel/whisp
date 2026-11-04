import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  currentMonitor,
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { playCue } from "../lib/sound";
import Mochi from "./Mochi";
import Waveform, { type WaveStatus } from "./Waveform";

type RecorderStatus = WaveStatus;

const BACKEND_MODES = ["Message", "Super", "Mail", "Note", "Voice", "Meeting"];

export type PopupPosition = "top-center" | "center" | "bottom-center" | "cursor";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function safeInvoke(cmd: string): Promise<void> {
  try {
    await invoke(cmd);
  } catch {
    /* browser dev without backend */
  }
}

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function applyThemeFromStorage(): void {
  // Backend owns `theme` when reachable; the settings window pushes it on
  // change, so localStorage is already in sync here.
  const theme = readStored("whisp:theme") ?? "system";
  document.documentElement.dataset.theme = theme;
}
/** Reposition the recorder window per the backend config (fallback: storage). */
async function applyPopupPosition(): Promise<void> {
  if (!isTauri()) return;
  let pos: PopupPosition = "top-center";
  const stored = readStored("whisp:popup-pos");
  if (stored === "center" || stored === "bottom-center" || stored === "cursor") pos = stored;
  // Backend config is source of truth when reachable.
  try {
    const cfg = await invoke<{ popupPos?: PopupPosition }>("get_config");
    if (
      cfg?.popupPos === "top-center" ||
      cfg?.popupPos === "center" ||
      cfg?.popupPos === "bottom-center" ||
      cfg?.popupPos === "cursor"
    ) {
      pos = cfg.popupPos;
    }
  } catch {
    /* backend en cours : localStorage seul */
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

function toStatus(raw: unknown): RecorderStatus {
  if (raw === "idle" || raw === "recording" || raw === "processing" || raw === "done" || raw === "error") {
    return raw;
  }
  return "recording";
}

function parseState(payload: unknown): { status: RecorderStatus; message?: string; mode?: string } {
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

function parseLevel(payload: unknown): number {
  if (typeof payload === "number" && Number.isFinite(payload)) {
    return Math.max(0, Math.min(1, payload));
  }
  if (payload && typeof payload === "object") {
    const v = (payload as Record<string, unknown>).level;
    if (typeof v === "number" && Number.isFinite(v)) return Math.max(0, Math.min(1, v));
  }
  return 0;
}

const MIC_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
    <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
    <line x1="12" x2="12" y1="19" y2="22" />
  </svg>
);

const SHRINK_ICON = (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="m14 10 7-7" />
    <path d="M20 10V4h-6" />
    <path d="m10 14-7 7" />
    <path d="M4 14v6h6" />
  </svg>
);

const EXPAND_ICON = (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M15 3h6v6" />
    <path d="m21 3-7 7" />
    <path d="m9 21-7-7" />
    <path d="M3 15v6h6" />
  </svg>
);

export default function Recorder() {
  const [status, setStatus] = useState<RecorderStatus>("recording");
  const [mode, setMode] = useState<string>("Auto");
  const [modeIndex, setModeIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [mini, setMini] = useState(false);
  const levelRef = useRef(0);
  const [partial, setPartial] = useState<string | null>(null);

  useEffect(() => {
    applyThemeFromStorage();
    void applyPopupPosition();
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlistens: Array<() => void> = [];
    void (async () => {
      try {
        const offLevel = await listen<unknown>("whisp://level", (e) => {
          if (!cancelled) levelRef.current = parseLevel(e.payload);
        });
        const offState = await listen<unknown>("whisp://state", (e) => {
          if (cancelled) return;
          const s = parseState(e.payload);
          setStatus((prev) => {
            if (prev === "recording" && s.status === "processing") playCue("stop");
            else if (prev === "processing" && s.status === "done") playCue("finish");
            else if (s.status === "error") playCue("error");
            return s.status;
          });
          setMessage(s.message ?? null);
          if (s.mode) setMode(s.mode);
          if (s.status !== "recording") setPartial(null);
        });
        const offPartial = await listen<unknown>("whisp://partial", (e) => {
          if (cancelled) return;
          const t =
            typeof e.payload === "string"
              ? e.payload
              : typeof (e.payload as Record<string, unknown> | null)?.text === "string"
                ? ((e.payload as Record<string, unknown>).text as string)
                : null;
          if (t && t.trim()) setPartial(t.trim());
        });
        unlistens.push(offLevel, offState, offPartial);
      } catch {
        /* browser dev without Tauri runtime */
      }
    })();
    return () => {
      cancelled = true;
      for (const u of unlistens) u();
    };
  }, []);

  // BIP de démarrage : la popup = le début d'une dictée.
  useEffect(() => {
    playCue("start");
  }, []);

  useEffect(() => {
    if (isTauri()) return;
    setStatus("recording");
    const id = window.setInterval(() => {
      const t = performance.now() / 1000;
      const v =
        0.4 +
        0.45 * Math.abs(Math.sin(t * 2.1)) * (0.4 + 0.6 * Math.abs(Math.sin(t * 0.63 + 1.2))) +
        Math.random() * 0.06;
      levelRef.current = Math.min(1, v);
    }, 50);
    return () => window.clearInterval(id);
  }, []);

  // Green flash then auto-hide once transcription is done.
  useEffect(() => {
    if (status !== "done") return;
    const id = window.setTimeout(() => {
      getCurrentWindow().hide().catch(() => {
        /* browser dev: nothing to hide */
      });
    }, 600);
    return () => window.clearTimeout(id);
  }, [status]);

  // Local mode cycling fallback when no backend answers cycle_mode.
  useEffect(() => {
    if (!isTauri()) setMode(BACKEND_MODES[modeIndex] ?? "Message");
  }, [modeIndex]);

  const stop = useCallback(() => {
    void safeInvoke("stop_recording");
  }, []);

  const cancel = useCallback(() => {
    void safeInvoke("cancel_recording");
  }, []);

  const cycleMode = useCallback(() => {
    void safeInvoke("cycle_mode");
    if (!isTauri()) setModeIndex((i) => (i + 1) % BACKEND_MODES.length);
  }, []);

  const toggleSize = useCallback(() => {
    const next = !mini;
    setMini(next);
    getCurrentWindow()
      .setSize(new LogicalSize(next ? 420 : 720, next ? 116 : 200))
      .catch(() => {
        /* browser dev: CSS-only shrink */
      });
  }, [mini]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      } else if (e.altKey && (e.code === "Space" || e.key === " ")) {
        e.preventDefault();
        stop();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [stop, cancel]);

  const cardClass =
    `recorder-card${mini ? " mini" : ""}` +
    ` is-${status}` +
    (status === "done" ? " flash-done" : "") +
    (status === "error" ? " flash-error" : "");

  return (
    <div className="recorder-stage">
      <div className={cardClass} role="dialog" aria-label="whisp recorder">
        <div className="recorder-top">
          <Mochi
            mood={status === "recording" ? "working" : status === "processing" ? "thinking" : status === "done" ? "finished" : status === "error" ? "error" : "idle"}
            size={24}
            level={levelRef}
          />
          <button
            type="button"
            className="resize-btn"
            onClick={toggleSize}
            title={mini ? "Agrandir" : "Réduire"}
            aria-label={mini ? "Agrandir la fenêtre" : "Réduire la fenêtre"}
          >
            {mini ? EXPAND_ICON : SHRINK_ICON}
          </button>
          <div className="wave-wrap">
            <Waveform level={levelRef} status={status} />
          </div>
          {status === "recording" && partial ? (
            <p className="recorder-partial" aria-live="polite">
              {partial}
            </p>
          ) : null}
          {status === "error" && message ? (
            <p className="recorder-error" role="alert">
              {message}
            </p>
          ) : null}
        </div>
        <div className="recorder-bar">
          <div className="bar-left">
            <button type="button" className="mode-btn" onClick={cycleMode} title="Changer de mode">
              {status === "recording" ? (
                <span className="rec-dot" aria-label="Enregistrement en cours" />
              ) : (
                <span className="mic">{MIC_ICON}</span>
              )}
              <span className="mode-name">{mode}</span>
            </button>
            {status === "processing" ? (
              <span className="status-line" aria-live="polite">
                <span className="dots" aria-hidden="true">
                  <i />
                  <i />
                  <i />
                </span>
                <span>Transcription…</span>
              </span>
            ) : null}
          </div>
          <div className="recorder-actions">
            <button type="button" className="rec-btn stop" onClick={stop}>
              Stop <kbd>Alt</kbd>
              <kbd>Espace</kbd>
            </button>
            <button type="button" className="rec-btn cancel" onClick={cancel}>
              Cancel <kbd>Échap</kbd>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
