import type { BotEngine } from "./botEngine";
import type { MochiMood, OutfitId } from "./types";
import { drawBody, drawEyes, eyeFrames, makeHead, REAL_OUTFITS } from "./realEngine";

/**
 * Moteur Mochi — composite fidèle à la référence coucou (realEngine.ts) :
 * dos accessoire -> corps superellipsoïde -> yeux -> devant accessoire,
 * plus bouche/états (moods) pour le feedback whisp (niveau audio, erreur…).
 * L'animation est pilotée par `Mochi.tsx`.
 */

export interface MochiFrame {
  mood: MochiMood;
  /** Niveau audio 0..1 (popup recorder) — fait rebondir / ouvrir la bouche. */
  level: number;
  /** Regard curseur -1..1 relatif au canvas (écouteur local au canvas). */
  gaze: { x: number; y: number };
  /** Secondes depuis le mount — pilote respiration/clignement. */
  t: number;
  size: number;
  outfit: OutfitId;
}

const INK = "#1A1412";

export function drawMochi(ctx: CanvasRenderingContext2D, f: MochiFrame): void {
  const { mood, level, gaze, t, size, outfit } = f;
  const s = size;
  ctx.clearRect(0, 0, s, s);

  // ── Mouvement du corps selon l'état ──────────────────────────────
  const u = s / 48; // unité : corps ≈ 48px
  let dy = Math.sin(t * 1.8) * 0.9 * u; // respiration (idle)
  let squash = 1;
  let shake = 0;
  if (mood === "working") {
    dy += Math.abs(Math.sin(t * 5)) * -2.2 * u; // rebond léger
    squash = 1 + Math.sin(t * 10) * 0.012;
  } else if (mood === "thinking") {
    dy += Math.sin(t * 3.2) * 1.2 * u;
  } else if (mood === "finished") {
    const k = t % 1.2;
    dy += k < 0.45 ? -Math.sin((k / 0.45) * Math.PI) * 5 * u : 0; // saut joie
  } else if (mood === "error") {
    shake = Math.sin(t * 40) * 1.4 * u; // secousse
  } else if (mood === "approval") {
    dy += Math.sin(t * 6) * 0.8 * u;
  }
  // Le niveau audio fait rebondir (popup recorder).
  dy += -Math.min(level, 1) * 2 * u;

  // ── Regard -> yaw/pitch tête ─────────────────────────────────────
  const gx = Math.max(-1, Math.min(1, gaze.x));
  const gy = Math.max(-1, Math.min(1, gaze.y));
  const yaw = mood === "thinking" ? Math.sin(t * 2.4) * 0.3 : gx * 0.35;
  const pitch = mood === "thinking" ? Math.cos(t * 1.7) * 0.15 : -gy * 0.25;

  const H = makeHead(s, {
    yaw,
    pitch,
    phys: {
      dx: mood === "working" ? Math.sin(t * 5) * 0.3 : Math.sin(t * 1.8) * 0.05,
      dy: Math.cos(t * 2.1) * 0.05,
    },
  });
  const o = outfit === "none" ? undefined : REAL_OUTFITS[outfit];

  ctx.save();
  ctx.translate(s / 2 + shake, s / 2 + dy);
  ctx.scale(1, squash);
  // Même cadrage que la référence : tête décalée de +R*0.45 (place aux chapeaux).
  ctx.translate(0, H.R * 0.45);
  if (o?.back) o.back(ctx, H);
  const path = drawBody(ctx, H, o?.bodyColors);
  const blink = mood === "idle" && t % 4 < 0.12;
  const closed = mood === "finished" || blink;
  const flat = mood === "error";
  if (closed || flat) {
    ctx.save();
    ctx.clip(path);
    for (const e of eyeFrames(H)) {
      if (!e.visible) continue;
      ctx.save();
      ctx.translate(e.x, e.y);
      ctx.scale(Math.max(0.3, e.fx), Math.max(0.3, e.fy));
      ctx.strokeStyle = INK;
      ctx.lineCap = "round";
      ctx.beginPath();
      if (closed) {
        ctx.lineWidth = H.R * 0.09;
        ctx.arc(0, H.R * 0.02, H.R * 0.11, Math.PI * 1.15, Math.PI * 1.85);
      } else {
        ctx.lineWidth = H.R * 0.1;
        ctx.moveTo(-H.R * 0.13, -H.R * 0.08);
        ctx.lineTo(H.R * 0.13, H.R * 0.04);
      }
      ctx.stroke();
      ctx.restore();
    }
    ctx.restore();
  } else {
    drawEyes(ctx, H, path);
  }

  // ── Bouche ─────────────────────────────────────────────────────
  const open = Math.min(level, 1) * H.R * 0.28 + (mood === "finished" ? H.R * 0.1 : 0);
  ctx.save();
  ctx.translate(yaw * H.rx * 0.3, H.ry * 0.42);
  if (open > H.R * 0.05) {
    ctx.fillStyle = INK;
    ctx.beginPath();
    ctx.ellipse(0, 0, H.R * 0.11, open / 2 + H.R * 0.03, 0, 0, Math.PI * 2);
    ctx.fill();
  } else {
    ctx.strokeStyle = INK;
    ctx.lineWidth = Math.max(1, H.R * 0.055);
    ctx.lineCap = "round";
    ctx.beginPath();
    if (mood === "error") ctx.arc(0, H.R * 0.14, H.R * 0.1, Math.PI * 1.2, Math.PI * 1.8);
    else ctx.arc(0, -H.R * 0.02, H.R * 0.1, Math.PI * 0.25, Math.PI * 0.75);
    ctx.stroke();
  }
  ctx.restore();

  if (o?.front) o.front(ctx, H, path);
  ctx.restore();
}

/**
 * Rendu piloté par BotEngine (animation coucou : springs, tweens, emotes).
 * Le DESSIN reste realEngine.ts ; l'animation vient du moteur.
 * `drawMochi` historique reste disponible (fallback statique).
 */
export function drawMochiWithEngine(
  ctx: CanvasRenderingContext2D,
  size: number,
  e: BotEngine,
  opts: { level: number; outfit: OutfitId },
): void {
  const s = size;
  ctx.clearRect(0, 0, s, s);
  const u = s / 48;
  const H = makeHead(s, {
    yaw: e.yaw,
    pitch: e.pitch,
    phys: { dx: e.physDx, dy: e.physDy },
  });
  const o = opts.outfit === "none" ? undefined : REAL_OUTFITS[opts.outfit];
  const shake = e.ox * H.R;
  const dy = e.oy * H.R + -Math.min(opts.level, 1) * 2 * u;
  ctx.save();
  ctx.translate(s / 2 + shake, s / 2 + dy);
  ctx.scale(e.sx, e.sy);
  if (e.tilt !== 0) ctx.rotate(e.tilt);
  ctx.translate(0, H.R * 0.45);
  ctx.save();
  if (o && e.outfitPresence > 0.01 && e.outfit !== "none") {
    ctx.globalAlpha = Math.max(0, Math.min(1, e.outfitPresence));
  }
  if (o?.back) o.back(ctx, H);
  const path = drawBody(ctx, H, o?.bodyColors);
  ctx.restore();
  // ── Yeux (état moteur : pill/wide/dot/flat/happy/closed/wink) ──
  ctx.save();
  ctx.clip(path);
  const eye = e.eye;
  for (const fr of eyeFrames(H)) {
    if (!fr.visible) continue;
    ctx.save();
    ctx.translate(fr.x, fr.y);
    ctx.scale(Math.max(0.3, fr.fx), Math.max(0.3, fr.fy));
    const w = fr.w * e.es;
    const h = fr.h * e.es;
    const isHappySide = eye === "wink" ? fr.sd > 0 : true;
    if ((eye === "happy" || (eye === "wink" && isHappySide)) || eye === "closed") {
      ctx.strokeStyle = INK;
      ctx.lineCap = "round";
      ctx.lineWidth = H.R * 0.09;
      ctx.beginPath();
      if (eye === "closed") {
        ctx.arc(0, -H.R * 0.02, H.R * 0.1, Math.PI * 0.2, Math.PI * 0.8);
      } else {
        ctx.arc(0, H.R * 0.02, H.R * 0.11, Math.PI * 1.15, Math.PI * 1.85);
      }
      ctx.stroke();
    } else if (eye === "flat") {
      ctx.strokeStyle = INK;
      ctx.lineCap = "round";
      ctx.lineWidth = H.R * 0.1;
      ctx.beginPath();
      ctx.moveTo(-H.R * 0.13, -H.R * 0.08);
      ctx.lineTo(H.R * 0.13, H.R * 0.04);
      ctx.stroke();
    } else if (eye === "dot") {
      ctx.fillStyle = INK;
      ctx.beginPath();
      ctx.arc(0, 0, w * 0.45, 0, Math.PI * 2);
      ctx.fill();
    } else {
      // pill | wide — wide = pill agrandi (yeux écarquillés, approval).
      const mw = eye === "wide" ? w * 1.16 : w;
      const mh = eye === "wide" ? h * 1.12 : h;
      const hh = Math.max(mh * e.open, mw * 0.3);
      ctx.fillStyle = INK;
      ctx.beginPath();
      const r = Math.min(mw / 2, hh / 2);
      ctx.moveTo(-mw / 2 + r, -hh / 2);
      ctx.arcTo(mw / 2, -hh / 2, mw / 2, hh / 2, r);
      ctx.arcTo(mw / 2, hh / 2, -mw / 2, hh / 2, r);
      ctx.arcTo(-mw / 2, hh / 2, -mw / 2, -hh / 2, r);
      ctx.arcTo(-mw / 2, -hh / 2, mw / 2, -hh / 2, r);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }
  ctx.restore();
  // ── Bouche ─────────────────────────────────────────────────────
  const open = Math.min(opts.level, 1) * H.R * 0.28 + (e.mood === "finished" ? H.R * 0.1 : 0);
  ctx.save();
  ctx.translate(e.yaw * H.rx * 0.3, H.ry * 0.42);
  if (open > H.R * 0.05) {
    ctx.fillStyle = INK;
    ctx.beginPath();
    ctx.ellipse(0, 0, H.R * 0.11, open / 2 + H.R * 0.03, 0, 0, Math.PI * 2);
    ctx.fill();
  } else {
    ctx.strokeStyle = INK;
    ctx.lineWidth = Math.max(1, H.R * 0.055);
    ctx.lineCap = "round";
    ctx.beginPath();
    if (e.mood === "error") ctx.arc(0, H.R * 0.14, H.R * 0.1, Math.PI * 1.2, Math.PI * 1.8);
    else ctx.arc(0, -H.R * 0.02, H.R * 0.1, Math.PI * 0.25, Math.PI * 0.75);
    ctx.stroke();
  }
  ctx.restore();
  if (o?.front) {
    ctx.save();
    if (e.outfit !== "none") ctx.globalAlpha = Math.max(0, Math.min(1, e.outfitPresence));
    o.front(ctx, H, path);
    ctx.restore();
  }
  ctx.restore();
}
