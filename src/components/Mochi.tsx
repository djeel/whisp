import { useEffect, useRef, useState } from "react";
import { BotEngine } from "../mochi/botEngine";
import { drawMochi, drawMochiWithEngine } from "../mochi/engine";
import { readOutfit, type MochiMood, type OutfitId } from "../mochi/types";

export type { MochiMood };

interface MochiProps {
  mood?: MochiMood;
  size?: number;
  /** Si faux : un seul rendu statique (settings header par ex.). */
  animate?: boolean;
  /** Source de niveau audio 0..1 (popup recorder) — ref mutable, sans re-render. */
  level?: { current: number };
  outfit?: OutfitId;
}

/**
 * Mochi — avatar canvas 2D piloté par BotEngine (port coucou).
 * Animation : update(dt) par frame + tweens/springs du moteur
 * (respiration idle, rebond working, scans thinking, joie finished).
 */
export default function Mochi({
  mood = "idle",
  size = 48,
  animate = true,
  level,
  outfit,
}: MochiProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const gazeRef = useRef({ x: 0, y: 0 });
  const moodRef = useRef(mood);
  const sizeRef = useRef(size);
  const outfitRef = useRef<OutfitId>(outfit ?? readOutfit());
  const engineRef = useRef<BotEngine | null>(null);
  const t0 = useRef(performance.now() / 1000);
  const [wardrobe, setWardrobe] = useState<OutfitId>(outfit ?? readOutfit());

  moodRef.current = mood;
  sizeRef.current = size;
  if (outfit !== undefined && outfit !== outfitRef.current) {
    outfitRef.current = outfit;
    setWardrobe(outfit);
  }

  // La garde-robe (settings) écrit localStorage ; on se resync au focus.
  useEffect(() => {
    if (outfit !== undefined) return;
    const sync = (): void => {
      const o = readOutfit();
      if (o !== outfitRef.current) {
        outfitRef.current = o;
        setWardrobe(o);
      }
    };
    window.addEventListener("focus", sync);
    window.addEventListener("storage", sync);
    return () => {
      window.removeEventListener("focus", sync);
      window.removeEventListener("storage", sync);
    };
  }, [outfit]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const s = sizeRef.current;
    canvas.width = Math.round(s * dpr);
    canvas.height = Math.round(s * dpr);

    if (!engineRef.current) engineRef.current = new BotEngine();
    const e = engineRef.current;
    e.setMood(moodRef.current);
    e.setOutfit(outfitRef.current, false);
    let last = performance.now() / 1000;

    const render = (): void => {
      const n = performance.now() / 1000;
      const dt = Math.min(0.1, Math.max(0.001, n - last));
      last = n;
      e.setMood(moodRef.current);
      e.setOutfit(outfitRef.current);
      e.lookX = Math.max(-1, Math.min(1, gazeRef.current.x));
      e.lookY = Math.max(-1, Math.min(1, gazeRef.current.y));
      e.update(dt);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawMochiWithEngine(ctx, sizeRef.current, e, {
        level: Math.max(0, Math.min(1, level?.current ?? 0)),
        outfit: outfitRef.current,
      });
    };

    // Premier rendu synchrone (même si animate=false).
    if (!animate) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawMochi(ctx, {
        mood: moodRef.current,
        level: 0,
        gaze: gazeRef.current,
        t: performance.now() / 1000 - t0.current,
        size: sizeRef.current,
        outfit: outfitRef.current,
      });
      return;
    }
    render();
    // Idle lent (8 fps), états actifs plus fluides (20 fps).
    const fps = mood === "idle" && level === undefined ? 125 : 50;
    const id = window.setInterval(render, fps);
    return () => window.clearInterval(id);
  }, [animate, level, mood, size, wardrobe]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <canvas
      ref={canvasRef}
      width={size}
      height={size}
      style={{ width: size, height: size, flexShrink: 0 }}
      role="img"
      aria-label={`Mochi (${mood})`}
      onMouseMove={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        gazeRef.current = {
          x: ((e.clientX - r.left) / Math.max(r.width, 1)) * 2 - 1,
          y: ((e.clientY - r.top) / Math.max(r.height, 1)) * 2 - 1,
        };
      }}
      onMouseLeave={() => {
        gazeRef.current = { x: 0, y: 0 };
      }}
    />
  );
}
