<script lang="ts">
  import { BotEngine } from "../mochi/botEngine";
  import { drawMochi, drawMochiWithEngine } from "../mochi/engine";
  import { readOutfit, type MochiMood, type OutfitId } from "../mochi/types";

  interface Props {
    mood?: MochiMood;
    size?: number;
    /** Si faux : un seul rendu statique (settings header par ex.). */
    animate?: boolean;
    /** Source de niveau audio 0..1 (popup recorder) — ref mutable, sans re-render. */
    level?: { current: number };
    outfit?: OutfitId;
  }

  let { mood = "idle", size = 48, animate = true, level, outfit }: Props = $props();

  export type { MochiMood };

  let canvas: HTMLCanvasElement | null = $state(null);
  // `key` du $effect animation : wardrobe change → boucle recréée (outfit switché).
  let wardrobeKey: OutfitId = $state(outfit ?? readOutfit());
  const gaze = { x: 0, y: 0 };
  const moodBox: { current: MochiMood } = { current: mood };
  const sizeBox: { current: number } = { current: size };
  const outfitBox: { current: OutfitId } = { current: outfit ?? readOutfit() };
  const t0 = performance.now() / 1000;
  let engine: BotEngine | null = null;

  $effect(() => {
    moodBox.current = mood;
  });
  $effect(() => {
    sizeBox.current = size;
  });
  $effect(() => {
    if (outfit !== undefined && outfit !== outfitBox.current) {
      outfitBox.current = outfit;
      wardrobeKey = outfit;
    }
  });

  // La garde-robe (settings) écrit localStorage ; on se resync au focus.
  $effect(() => {
    if (outfit !== undefined) return;
    const sync = (): void => {
      const o = readOutfit();
      if (o !== outfitBox.current) {
        outfitBox.current = o;
        wardrobeKey = o;
      }
    };
    window.addEventListener("focus", sync);
    window.addEventListener("storage", sync);
    return () => {
      window.removeEventListener("focus", sync);
      window.removeEventListener("storage", sync);
    };
  });

  $effect(() => {
    void wardrobeKey;
    const cv: HTMLCanvasElement | null = canvas;
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const s = sizeBox.current;
    cv.width = Math.round(s * dpr);
    cv.height = Math.round(s * dpr);

    if (!engine) engine = new BotEngine();
    const e = engine;
    e.setMood(moodBox.current);
    e.setOutfit(outfitBox.current, false);
    let last = performance.now() / 1000;

    const render = (): void => {
      const n = performance.now() / 1000;
      const dt = Math.min(0.1, Math.max(0.001, n - last));
      last = n;
      e.setMood(moodBox.current);
      e.setOutfit(outfitBox.current);
      e.lookX = Math.max(-1, Math.min(1, gaze.x));
      e.lookY = Math.max(-1, Math.min(1, gaze.y));
      e.update(dt);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawMochiWithEngine(ctx, sizeBox.current, e, {
        level: Math.max(0, Math.min(1, level?.current ?? 0)),
        outfit: outfitBox.current,
      });
    };

    // Premier rendu synchrone (même si animate=false).
    if (!animate) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawMochi(ctx, {
        mood: moodBox.current,
        level: 0,
        gaze,
        t: performance.now() / 1000 - t0,
        size: sizeBox.current,
        outfit: outfitBox.current,
      });
      return;
    }
    render();
    // La boucle tourne tant que le composant est monté : Mochi respire
    // (idle) et réagit (working/thinking) en continu dans la popup.
    // Seuls les headers settings (animate=false ou idle sans level)
    // restent en image fixe.
    if (!animate || (mood === "idle" && level === undefined)) return;
    const fps = 50;
    const id = window.setInterval(render, fps);
    return () => window.clearInterval(id);
  });
</script>

<canvas
  bind:this={canvas}
  width={size}
  height={size}
  style:width="{size}px"
  style:height="{size}px"
  style:flex-shrink="0"
  role="img"
  aria-label="Mochi ({mood})"
  onmousemove={(e) => {
    const r = e.currentTarget.getBoundingClientRect();
    gaze.x = ((e.clientX - r.left) / Math.max(r.width, 1)) * 2 - 1;
    gaze.y = ((e.clientY - r.top) / Math.max(r.height, 1)) * 2 - 1;
  }}
  onmouseleave={() => {
    gaze.x = 0;
    gaze.y = 0;
  }}
></canvas>
