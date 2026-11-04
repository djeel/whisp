import { useEffect, useRef } from "react";

export type WaveStatus = "idle" | "recording" | "processing" | "done" | "error";

interface WaveformProps {
  /** Mutable level source (0..1). Updated at any rate without re-rendering. */
  level: { current: number };
  status: WaveStatus;
  /** Target bar count (clamped to available width). */
  bars?: number;
}

const BAR_W = 3;
const GAP = 4;
const ATTACK = 0.5;
const RELEASE = 0.08;

function readVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

interface WavePalette { idle: string; active: string; done: string; error: string }

function readPalette(): WavePalette {
  return {
    idle: readVar("--wave-idle", "#9a9a9a"),
    active: readVar("--wave-active", "#ffffff"),
    done: readVar("--green", "#4ade80"),
    error: readVar("--red", "#ef4444"),
  };
}

function paletteFor(status: WaveStatus, p: WavePalette): string {
  switch (status) {
    case "recording": return p.active;
    case "processing": return p.idle;
    case "done": return p.done;
    case "error": return p.error;
    default: return p.idle;
  }
}

function drawBar(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
  color: string,
): void {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
  ctx.fill();
}

export default function Waveform({ level, status, bars = 120 }: WaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const statusRef = useRef<WaveStatus>(status);
  statusRef.current = status;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const values: number[] = new Array<number>(bars).fill(0.04);
    let raf = 0;
    let width = 0;
    let height = 0;

    const resize = (): void => {
      const dpr = Math.min(2, window.devicePixelRatio || 1);
      const rect = canvas.getBoundingClientRect();
      width = Math.max(1, Math.floor(rect.width));
      height = Math.max(1, Math.floor(rect.height));
      canvas.width = Math.max(1, Math.floor(rect.width * dpr));
      canvas.height = Math.max(1, Math.floor(rect.height * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const render = (now: number): void => {
      const t = now / 1000;
      const st = statusRef.current;
      const palette = readPalette();
      const color = paletteFor(st, palette);
      ctx.clearRect(0, 0, width, height);
      const mid = height / 2;

      if (st === "idle") {
        ctx.save();
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.setLineDash([2, 7]);
        ctx.beginPath();
        ctx.moveTo(0, mid);
        ctx.lineTo(width, mid);
        ctx.stroke();
        ctx.restore();
      } else {
        const step = BAR_W + GAP;
        const count = Math.min(bars, Math.max(8, Math.floor((width + GAP) / step)));
        const totalW = count * step - GAP;
        let x = (width - totalW) / 2;
        const base = st === "processing" ? 0 : Math.max(0, Math.min(1, level.current));
        for (let i = 0; i < count; i++) {
          let target: number;
          if (st === "processing") {
            target = 0.05 + 0.03 * Math.abs(Math.sin(t * 2 + i * 0.5));
          } else if (st === "done" || st === "error") {
            target = Math.max(0.03, (values[i] ?? 0.03) - 0.02);
          } else {
            const env =
              Math.abs(Math.sin(i * 0.55 + t * 3.1)) *
              (0.35 + 0.65 * Math.abs(Math.sin(i * 0.13 - t * 1.7)));
            // Gain visuel : le RMS voix (~0.1-0.4) remplit la hauteur.
            // sqrt = compression dynamique (faibles audibles, forts saturés doux).
            const boosted = Math.min(1, Math.sqrt(Math.max(0, base) * 2.5));
            target = Math.max(0.05, boosted * (0.35 + 0.65 * env));
          }
          const v = values[i] ?? 0.03;
          const k = target > v ? ATTACK : RELEASE;
          const nv = v + (target - v) * k;
          values[i] = nv;
          const h = Math.max(3, nv * (height - 8));
          drawBar(ctx, x, mid - h / 2, BAR_W, h, BAR_W / 2, color);
          x += step;
        }
      }
      raf = requestAnimationFrame(render);
    };
    raf = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [bars]);

  return <canvas ref={canvasRef} className="wave-canvas" aria-hidden="true" />;
}
