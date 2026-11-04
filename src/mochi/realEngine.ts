// Vrai renderer Mochi — port TypeScript fidèle de
// /tmp/coucou-study/design/outfits/mochi-outfits.js (référence coucou).
// Modèle : superellipsoïde, origine au centre du corps, y écran vers le bas,
// R = W*0.3, rx = 1.14R, ry = 0.88R.
export interface HeadPhys {
  dx: number;
  dy: number;
}

export interface Head {
  R: number;
  rx: number;
  ry: number;
  /** Inclinaison de vue (vue légèrement du dessus). */
  view: number;
  yaw: number;
  pitch: number;
  phys: HeadPhys;
}

export interface RealOpts {
  scale?: number;
  yaw?: number;
  pitch?: number;
  tilt?: number;
  phys?: HeadPhys;
  outfit?: string;
}

type V3 = [number, number, number];

const EXP = 2.7;
const VIEW_TILT = -0.3;
const ACC_PITCH = 0.4;
const EYE_W = 0.25;
const EYE_H = 0.27;
const EYE_SP = 0.37;
const EYE_P = -0.12;

function ringR(y: number): number {
  const a = Math.min(1, Math.abs(y));
  return Math.pow(1 - Math.pow(a, EXP), 1 / EXP);
}

/** Rotation d'un point head-local (x droite, y haut, z vers viewer), yaw puis pitch. */
function rot(p: V3, yaw: number, pitch: number): V3 {
  const [x, y, z] = p;
  const cy = Math.cos(yaw);
  const sy = Math.sin(yaw);
  const x1 = x * cy + z * sy;
  const z1 = -x * sy + z * cy;
  const cp = Math.cos(pitch);
  const sp = Math.sin(pitch);
  const y2 = y * cp + z1 * sp;
  const z2 = -y * sp + z1 * cp;
  return [x1, y2, z2];
}

export interface ScreenPt {
  x: number;
  y: number;
  z: number;
}

/** Head-local -> écran (espace corps). */
function proj(H: Head, p: V3): ScreenPt {
  const r = rot(p, H.yaw, H.view + H.pitch * ACC_PITCH);
  return { x: r[0] * H.rx, y: -r[1] * H.ry, z: r[2] };
}

/** Point sur la surface de la tête : hauteur y (-1..1), longitude lon (0 = face). */
function surf(y: number, lon: number, s = 1): V3 {
  const r = ringR(y) * s;
  return [r * Math.sin(lon), y, r * Math.cos(lon)];
}

export function mochiPath(rx: number, ry: number): Path2D {
  const p = new Path2D();
  const n = 96;
  const e = 2 / EXP;
  for (let i = 0; i <= n; i++) {
    const a = (i / n) * Math.PI * 2;
    const ca = Math.cos(a);
    const sa = Math.sin(a);
    const x = rx * Math.sign(ca) * Math.pow(Math.abs(ca), e);
    const y = ry * Math.sign(sa) * Math.pow(Math.abs(sa), e);
    if (i === 0) p.moveTo(x, y);
    else p.lineTo(x, y);
  }
  p.closePath();
  return p;
}

function lin(
  ctx: CanvasRenderingContext2D,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  stops: Array<[number, string]>,
): CanvasGradient {
  const g = ctx.createLinearGradient(x0, y0, x1, y1);
  for (const [o, c] of stops) g.addColorStop(o, c);
  return g;
}

function rad(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  r0: number,
  r1: number,
  stops: Array<[number, string]>,
): CanvasGradient {
  const g = ctx.createRadialGradient(x, y, r0, x, y, r1);
  for (const [o, c] of stops) g.addColorStop(o, c);
  return g;
}

// ---------- Corps + yeux (fidèle au BotEngine) ----------
export function drawBody(
  ctx: CanvasRenderingContext2D,
  H: Head,
  colors?: [string, string],
): Path2D {
  const { rx, ry, R } = H;
  const path = mochiPath(rx, ry);
  const top = colors ? colors[0] : "#EDEDEF";
  const bot = colors ? colors[1] : "#C4C5CA";
  ctx.fillStyle = lin(ctx, rx * 0.7, -ry * 0.85, -rx * 0.8, ry * 0.9, [
    [0, top],
    [1, bot],
  ]);
  ctx.fill(path);
  ctx.fillStyle = rad(ctx, 0, 0, R * 0.15, R * 1.25, [
    [0, "rgba(0,0,0,0)"],
    [0.6, "rgba(0,0,0,0)"],
    [1, "rgba(0,0,0,0.2)"],
  ]);
  ctx.fill(path);
  ctx.fillStyle = rad(ctx, rx * 0.34, -ry * 0.46, 0, R * 0.42, [
    [0, "rgba(255,255,255,0.55)"],
    [1, "rgba(255,255,255,0)"],
  ]);
  ctx.fill(path);
  return path;
}

export interface EyeFrame {
  sd: number;
  visible: boolean;
  x: number;
  y: number;
  fx: number;
  fy: number;
  w: number;
  h: number;
}

/** Addition sphérique (PAS la rotation des accessoires). */
export function eyeFrames(H: Head): EyeFrame[] {
  const out: EyeFrame[] = [];
  for (const sd of [-1, 1]) {
    const eyeYaw = sd * EYE_SP + H.yaw;
    const eyePitch = EYE_P + H.pitch;
    const cp = Math.cos(eyePitch);
    const visible = Math.cos(eyeYaw) * cp > 0.04;
    out.push({
      sd,
      visible,
      x: Math.sin(eyeYaw) * cp * H.rx,
      y: -Math.sin(eyePitch) * H.ry,
      fx: Math.max(0.18, Math.cos(eyeYaw)),
      fy: Math.max(0.18, cp),
      w: H.R * EYE_W,
      h: H.R * EYE_H,
    });
  }
  return out;
}

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

export function drawEyes(
  ctx: CanvasRenderingContext2D,
  H: Head,
  path: Path2D,
  alpha = 1,
): void {
  ctx.save();
  ctx.clip(path);
  for (const e of eyeFrames(H)) {
    if (!e.visible) continue;
    ctx.save();
    ctx.translate(e.x, e.y);
    ctx.scale(e.fx, e.fy);
    const w = e.w;
    const hh = e.h;
    ctx.fillStyle = `rgba(26,20,18,${alpha})`;
    roundRectPath(ctx, -w / 2, -hh / 2, w, hh, Math.min(w, hh) / 2);
    ctx.fill();
    ctx.restore();
  }
  ctx.restore();
}

// ---------- utilitaires "wrapped" ----------
interface RingPt extends ScreenPt {
  lon: number;
}

function ringPoints(H: Head, y: number, s: number, n = 72): RingPt[] {
  const pts: RingPt[] = [];
  for (let i = 0; i <= n; i++) {
    const lon = -Math.PI + (i / n) * 2 * Math.PI;
    pts.push({ lon, ...proj(H, surf(y, lon, s)) });
  }
  return pts;
}

function frontArc(H: Head, y: number, s: number): ScreenPt[] {
  const pts = ringPoints(H, y, s, 120).filter((p) => p.z >= -0.02);
  pts.sort((a, b) => a.x - b.x);
  return pts;
}

/** Région gonflée AU-DESSUS de l'arc avant de l'anneau y (ce qu'une casquette couvre). */
function capClip(H: Head, y: number, s: number, extraTop = 3): Path2D {
  const arc = frontArc(H, y, s);
  const p = new Path2D();
  p.moveTo(arc[0].x - H.rx, arc[0].y);
  for (const q of arc) p.lineTo(q.x, q.y);
  p.lineTo(arc[arc.length - 1].x + H.rx, arc[arc.length - 1].y);
  p.lineTo(H.rx * 2, -H.ry * extraTop);
  p.lineTo(-H.rx * 2, -H.ry * extraTop);
  p.closePath();
  return p;
}


function strokeArc(ctx: CanvasRenderingContext2D, arc: ScreenPt[]): void {
  ctx.beginPath();
  arc.forEach((q, i) => {
    if (i === 0) ctx.moveTo(q.x, q.y);
    else ctx.lineTo(q.x, q.y);
  });
}

/** Complément de région (grand rect moins path) via evenodd — sans patcher le prototype. */
function invertClip(H: Head, p: Path2D): Path2D {
  const q = new Path2D();
  q.rect(-H.rx * 4, -H.ry * 4, H.rx * 8, H.ry * 8);
  q.addPath(p);
  return q;
}

function clipEvenOdd(ctx: CanvasRenderingContext2D, p: Path2D): void {
  ctx.clip(p, "evenodd");
}

/** Pompon doux en touffes qui se chevauchent. */
function pompom(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  r: number,
  base = "#FFFFFF",
  shade = "#D5D9E2",
): void {
  ctx.save();
  ctx.translate(x, y);
  const n = 11;
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2;
    const br = r * (0.34 + 0.06 * Math.sin(i * 2.3));
    const bx = Math.cos(a) * r * 0.78;
    const by = Math.sin(a) * r * 0.78;
    ctx.fillStyle = rad(ctx, bx - br * 0.4, by - br * 0.5, 0, br * 1.3, [
      [0, base],
      [1, shade],
    ]);
    ctx.beginPath();
    ctx.arc(bx, by, br, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.fillStyle = rad(ctx, -r * 0.3, -r * 0.35, 0, r * 1.05, [
    [0, base],
    [0.7, base],
    [1, shade],
  ]);
  ctx.beginPath();
  ctx.arc(0, 0, r * 0.86, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** Bande duveteuse le long d'une polyligne (trim du bonnet de Noël). */
function fuzzyBand(
  ctx: CanvasRenderingContext2D,
  arc: ScreenPt[],
  thick: number,
  base = "#FFFFFF",
  shade = "#DADDE4",
): void {
  ctx.save();
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  strokeArc(ctx, arc);
  ctx.strokeStyle = shade;
  ctx.lineWidth = thick;
  ctx.stroke();
  strokeArc(ctx, arc);
  ctx.strokeStyle = base;
  ctx.lineWidth = thick * 0.78;
  ctx.stroke();
  const step = Math.max(2, Math.floor(arc.length / 16));
  for (let i = 0; i < arc.length; i += step) {
    const q = arc[i];
    const r = thick * (0.32 + 0.1 * Math.sin(i * 1.7));
    ctx.fillStyle = rad(ctx, q.x - r * 0.3, q.y - thick * 0.35 - r * 0.3, 0, r * 1.2, [
      [0, base],
      [1, shade],
    ]);
    ctx.beginPath();
    ctx.arc(q.x, q.y - thick * 0.32, r, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.restore();
}


// ---------- TENUES ----------
export interface RealOutfit {
  back?: (ctx: CanvasRenderingContext2D, H: Head) => void;
  front?: (ctx: CanvasRenderingContext2D, H: Head, path: Path2D) => void;
  frontAfterEyes?: boolean;
  bodyColors?: [string, string];
}

type Ctx = CanvasRenderingContext2D;

function beanieFront(ctx: Ctx, H: Head, path: Path2D): void {
  const s = 1.035;
  const yEdge = 0.42;
  const yCuff = 0.58;
  const head = mochiPath(H.rx * s, H.ry * s);
  // Ombre sur la tête sous le revers.
  ctx.save();
  ctx.clip(path);
  ctx.clip(capClip(H, yEdge - 0.12, 1));
  ctx.fillStyle = "rgba(30,40,70,0.10)";
  ctx.fill(path);
  ctx.restore();
  // Corps tricoté.
  ctx.save();
  ctx.clip(capClip(H, yCuff, s));
  ctx.fillStyle = lin(ctx, H.rx * 0.5, -H.ry * 1.1, -H.rx * 0.6, H.ry * 0.2, [
    [0, "#7DB6FF"],
    [1, "#2F6FE0"],
  ]);
  ctx.fill(head);
  // Côtes verticales suivant les méridiens.
  ctx.clip(head);
  for (let k = -6; k <= 6; k++) {
    const lon = k * 0.24;
    const pts: ScreenPt[] = [];
    for (let i = 0; i <= 16; i++) {
      const y = yCuff + ((1.05 - yCuff) * i) / 16;
      const q = proj(H, surf(y, lon, s));
      if (q.z > 0) pts.push(q);
    }
    if (pts.length < 2) continue;
    ctx.beginPath();
    pts.forEach((q, i) => {
      if (i === 0) ctx.moveTo(q.x, q.y);
      else ctx.lineTo(q.x, q.y);
    });
    ctx.strokeStyle = "rgba(20,50,140,0.16)";
    ctx.lineWidth = H.R * 0.045;
    ctx.stroke();
  }
  ctx.restore();
  // Revers (bandeau plié).
  ctx.save();
  ctx.clip(capClip(H, yEdge, s * 1.04));
  clipEvenOdd(ctx, invertClip(H, capClip(H, yCuff, s * 1.04)));
  const cuffHead = mochiPath(H.rx * s * 1.04, H.ry * s * 1.04);
  ctx.fillStyle = lin(ctx, 0, -H.ry * 0.6, 0, -H.ry * 0.2, [
    [0, "#3C7BEA"],
    [1, "#2257C4"],
  ]);
  ctx.fill(cuffHead);
  ctx.clip(cuffHead);
  for (let k = -14; k <= 14; k++) {
    const lon = k * 0.115;
    const a = proj(H, surf(yEdge, lon, s * 1.04));
    const b = proj(H, surf(yCuff, lon, s * 1.04));
    if (a.z < 0) continue;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.strokeStyle = "rgba(10,30,100,0.22)";
    ctx.lineWidth = H.R * 0.035;
    ctx.stroke();
  }
  ctx.restore();
  // Reflet du dessus.
  ctx.save();
  ctx.clip(capClip(H, yCuff, s));
  ctx.clip(head);
  ctx.fillStyle = rad(ctx, H.rx * 0.3, -H.ry * 0.85, 0, H.R * 0.45, [
    [0, "rgba(255,255,255,0.35)"],
    [1, "rgba(255,255,255,0)"],
  ]);
  ctx.fill(head);
  ctx.restore();
  // Pompon sur petit ressort.
  const top = proj(H, [0, 1.08 * s, 0]);
  pompom(
    ctx,
    top.x + H.phys.dx * H.rx * 0.25,
    top.y - H.R * 0.12 + H.phys.dy * H.ry * 0.15,
    H.R * 0.24,
  );
}

function santaFront(ctx: Ctx, H: Head, path: Path2D): void {
  const s = 1.05;
  const yEdge = 0.52;
  const arc = frontArc(H, yEdge, s);
  const L = arc[0];
  const Rt = arc[arc.length - 1];
  const crown = proj(H, [0, 1.05, 0]);
  // Pointe : retombe à droite, plus ressort.
  const side = 1;
  const tip = {
    x: crown.x + side * H.rx * (0.95 + H.phys.dx * 0.35),
    y: crown.y + H.ry * (0.05 + H.phys.dy * 0.2),
  };
  const peak = { x: crown.x + side * H.rx * 0.25, y: crown.y - H.ry * 0.62 };
  const bag = new Path2D();
  bag.moveTo(L.x, L.y);
  bag.bezierCurveTo(
    L.x - H.rx * 0.05,
    L.y - H.ry * 0.7,
    peak.x - H.rx * 0.55,
    peak.y - H.ry * 0.05,
    peak.x,
    peak.y,
  );
  bag.quadraticCurveTo(tip.x - H.rx * 0.05, peak.y - H.ry * 0.02, tip.x, tip.y);
  bag.quadraticCurveTo(
    tip.x - H.rx * 0.12,
    tip.y - H.ry * 0.22,
    peak.x + H.rx * 0.18,
    peak.y + H.ry * 0.32,
  );
  bag.bezierCurveTo(
    Rt.x + H.rx * 0.05,
    peak.y + H.ry * 0.45,
    Rt.x + H.rx * 0.08,
    Rt.y - H.ry * 0.35,
    Rt.x,
    Rt.y,
  );
  for (let i = arc.length - 1; i >= 0; i--) bag.lineTo(arc[i].x, arc[i].y);
  bag.closePath();
  // Ombre sur la tête.
  ctx.save();
  ctx.clip(path);
  ctx.clip(capClip(H, yEdge - 0.14, 1));
  ctx.fillStyle = "rgba(120,10,10,0.10)";
  ctx.fill(path);
  ctx.restore();
  ctx.fillStyle = lin(ctx, -H.rx * 0.6, -H.ry * 1.6, H.rx * 0.7, -H.ry * 0.3, [
    [0, "#FF6B6B"],
    [0.55, "#E53935"],
    [1, "#B71C1C"],
  ]);
  ctx.fill(bag);
  // Plis : creux doux suivant le tombé.
  ctx.save();
  ctx.clip(bag);
  ctx.lineCap = "round";
  const folds: Array<[number, number, number]> = [
    [0.15, 0.55, 0.1],
    [0.45, 0.85, 0.08],
  ];
  for (const [a, b, w] of folds) {
    ctx.beginPath();
    ctx.moveTo(peak.x - H.rx * 0.1 + ((Rt.x - L.x) * a * 0.3), peak.y + H.ry * 0.15);
    ctx.quadraticCurveTo(
      peak.x + H.rx * 0.35,
      peak.y + H.ry * (0.05 + a * 0.3),
      tip.x - H.rx * (0.45 - b * 0.3),
      tip.y - H.ry * 0.12,
    );
    ctx.strokeStyle = "rgba(90,0,0,0.20)";
    ctx.lineWidth = H.R * w;
    ctx.stroke();
  }
  ctx.fillStyle = rad(ctx, peak.x - H.rx * 0.25, peak.y + H.ry * 0.05, 0, H.R * 0.5, [
    [0, "rgba(255,255,255,0.32)"],
    [1, "rgba(255,255,255,0)"],
  ]);
  ctx.fill(bag);
  ctx.restore();
  // Trim + pompon.
  fuzzyBand(ctx, arc, H.R * 0.3);
  pompom(ctx, tip.x, tip.y + H.R * 0.04, H.R * 0.22);
}

// --- Couronne : bandeau doré AUTOUR de la tête, pics tout autour ---
function crownGeom(H: Head): { s: number; yb: number; yt: number; n: number; spikeH: number } {
  void H;
  return { s: 1.06, yb: 0.46, yt: 0.66, n: 8, spikeH: 0.42 };
}

function crownPart(ctx: Ctx, H: Head, side: -1 | 1): void {
  const { s, yb, yt, n, spikeH } = crownGeom(H);
  const N = 120;
  const seg: Array<{ lon: number; b: ScreenPt; t: ScreenPt; tt: ScreenPt; z: number }> = [];
  for (let i = 0; i <= N; i++) {
    const lon = -Math.PI + (i / N) * 2 * Math.PI;
    const b = proj(H, surf(yb, lon, s));
    const t = proj(H, surf(yt, lon, s));
    const phase = ((lon + Math.PI) / (2 * Math.PI)) * n;
    const f = phase - Math.floor(phase);
    const spike = Math.pow(Math.max(0, 1 - Math.abs(f - 0.5) * 2), 1.6);
    const topY = yt + spikeH * spike;
    const base = surf(yt, lon, s);
    const tt = proj(H, [base[0] * (1 - 0.08 * spike), topY, base[2] * (1 - 0.08 * spike)]);
    seg.push({ lon, b, t, tt, z: b.z });
  }
  void seg[0].t;
  const keep = seg.filter((q) => (side > 0 ? q.z >= 0 : q.z < 0.02));
  if (keep.length < 2) return;
  keep.sort((a, b) => a.b.x - b.b.x);
  const shape = new Path2D();
  keep.forEach((q, i) => {
    if (i === 0) shape.moveTo(q.tt.x, q.tt.y);
    else shape.lineTo(q.tt.x, q.tt.y);
  });
  for (let i = keep.length - 1; i >= 0; i--) shape.lineTo(keep[i].b.x, keep[i].b.y);
  shape.closePath();
  const dark = side < 0;
  ctx.fillStyle = lin(ctx, 0, -H.ry * 1.05, 0, -H.ry * 0.45, dark ? [[0, "#C98A12"], [1, "#8A5A06"]] : [[0, "#FFE58A"], [0.5, "#FBBF24"], [1, "#D08A0B"]]);
  ctx.fill(shape);
  if (!dark) {
    ctx.save();
    ctx.clip(shape);
    ctx.fillStyle = lin(ctx, -H.rx, 0, H.rx, 0, [
      [0, "rgba(120,70,0,0.25)"],
      [0.45, "rgba(255,255,255,0.0)"],
      [0.62, "rgba(255,255,255,0.35)"],
      [1, "rgba(120,70,0,0.25)"],
    ]);
    ctx.fill(shape);
    ctx.restore();
    const gems = ["#EF4444", "#3B82F6", "#22C55E", "#A855F7"];
    for (let k = 0; k < n; k++) {
      const lon = -Math.PI + ((k + 0.5) / n) * 2 * Math.PI;
      const sp = surf(yt, lon, s);
      const tipP = proj(H, [sp[0] * 0.92, yt + spikeH, sp[2] * 0.92]);
      const mid = proj(H, surf((yb + yt) / 2, lon, s * 1.01));
      if (mid.z <= 0.12) continue;
      const r = H.R * 0.055;
      ctx.beginPath();
      ctx.arc(tipP.x, tipP.y - r * 0.5, r, 0, Math.PI * 2);
      ctx.fillStyle = rad(ctx, tipP.x - r * 0.3, tipP.y - r, 0, r * 1.2, [
        [0, "#FFF6CC"],
        [1, "#E0A21A"],
      ]);
      ctx.fill();
      const gr = H.R * 0.075;
      ctx.beginPath();
      ctx.ellipse(mid.x, mid.y, gr * Math.max(0.35, mid.z), gr, 0, 0, Math.PI * 2);
      ctx.fillStyle = gems[k % gems.length];
      ctx.fill();
      ctx.beginPath();
      ctx.arc(mid.x - gr * 0.25 * mid.z, mid.y - gr * 0.35, gr * 0.28, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(255,255,255,0.75)";
      ctx.fill();
    }
  }
}

// --- Lunettes de soleil ancrées aux vraies positions des yeux ---
function lens(ctx: Ctx, e: EyeFrame, w: number, h: number, r: number): void {
  ctx.save();
  ctx.translate(e.x, e.y);
  ctx.scale(e.fx, e.fy);
  roundRectPath(ctx, -w / 2, -h / 2, w, h, r);
  ctx.restore();
}

function sunglassesFront(ctx: Ctx, H: Head, path: Path2D): void {
  const eyes = eyeFrames(H);
  const w = H.R * 0.62;
  const h = H.R * 0.46;
  ctx.save();
  ctx.clip(path);
  const [l, r] = eyes;
  if (l.visible && r.visible) {
    ctx.beginPath();
    ctx.moveTo(l.x + ((w / 2) * l.fx * 0.9), l.y - h * 0.18);
    ctx.quadraticCurveTo((l.x + r.x) / 2, (l.y + r.y) / 2 - h * 0.42, r.x - ((w / 2) * r.fx * 0.9), r.y - h * 0.18);
    ctx.strokeStyle = "#111317";
    ctx.lineWidth = H.R * 0.07;
    ctx.stroke();
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    const ox = e.x + (e.sd * w) / 2 * e.fx;
    ctx.beginPath();
    ctx.moveTo(ox, e.y - h * 0.2);
    ctx.lineTo(e.sd * H.rx * 1.05, e.y - h * 0.35);
    ctx.strokeStyle = "#111317";
    ctx.lineWidth = H.R * 0.06;
    ctx.stroke();
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    lens(ctx, e, w, h, h * 0.42);
    ctx.fillStyle = "rgba(17,19,23,0.82)";
    ctx.fill();
    ctx.lineWidth = H.R * 0.05;
    ctx.strokeStyle = "#0B0C0F";
    ctx.stroke();
    ctx.save();
    ctx.translate(e.x, e.y);
    ctx.scale(e.fx, e.fy);
    ctx.beginPath();
    ctx.moveTo(-w * 0.28, -h * 0.05);
    ctx.lineTo(-w * 0.05, -h * 0.3);
    ctx.strokeStyle = "rgba(255,255,255,0.45)";
    ctx.lineWidth = H.R * 0.05;
    ctx.lineCap = "round";
    ctx.stroke();
    ctx.restore();
  }
  ctx.restore();
}

// --- Écharpe : bande tricotée en bas du corps, nœud + pan qui pend ---
function scarfFront(ctx: Ctx, H: Head, path: Path2D): void {
  void path;
  const s = 1.05;
  const y0 = -0.34;
  const y1 = -0.66;
  const top = frontArc(H, y0, s);
  const bot = frontArc(H, y1, s);
  const band = new Path2D();
  top.forEach((q, i) => {
    if (i === 0) band.moveTo(q.x, q.y);
    else band.lineTo(q.x, q.y);
  });
  for (let i = bot.length - 1; i >= 0; i--) band.lineTo(bot[i].x, bot[i].y);
  band.closePath();
  ctx.save();
  ctx.clip(mochiPath(H.rx * s, H.ry * s));
  ctx.fillStyle = lin(ctx, 0, -H.ry * 0.2, 0, H.ry * 0.7, [
    [0, "#F87171"],
    [1, "#B91C1C"],
  ]);
  ctx.fill(band);
  ctx.clip(band);
  // Rayures le long des méridiens.
  for (const lon of [-1.0, -0.45, 0.1, 0.65, 1.2]) {
    const a = proj(H, surf(y0, lon, s));
    const b = proj(H, surf(y1, lon, s));
    if (a.z < 0) continue;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y - 4);
    ctx.lineTo(b.x, b.y + 4);
    ctx.strokeStyle = "rgba(255,255,255,0.85)";
    ctx.lineWidth = H.R * 0.09 * Math.max(0.3, a.z);
    ctx.stroke();
  }
  ctx.fillStyle = lin(ctx, 0, -H.ry * 0.5, 0, H.ry * 0.3, [
    [0, "rgba(255,255,255,0.18)"],
    [1, "rgba(0,0,0,0.1)"],
  ]);
  ctx.fill(band);
  ctx.restore();
  // Pan pendant depuis le nœud (lon -0.55).
  const k = proj(H, surf((y0 + y1) / 2, -0.55, s * 1.03));
  if (k.z > 0) {
    const sw = H.phys.dx * H.rx * 0.12;
    const end = new Path2D();
    end.moveTo(k.x - H.R * 0.16, k.y);
    end.quadraticCurveTo(k.x - H.R * 0.24 + sw, k.y + H.ry * 0.35, k.x - H.R * 0.2 + sw * 1.4, k.y + H.ry * 0.62);
    end.lineTo(k.x + H.R * 0.06 + sw * 1.4, k.y + H.ry * 0.6);
    end.quadraticCurveTo(k.x + H.R * 0.02 + sw, k.y + H.ry * 0.3, k.x + H.R * 0.12, k.y);
    end.closePath();
    ctx.fillStyle = lin(ctx, 0, k.y, 0, k.y + H.ry * 0.6, [
      [0, "#EF4444"],
      [1, "#B91C1C"],
    ]);
    ctx.fill(end);
    ctx.save();
    ctx.clip(end);
    ctx.fillStyle = "rgba(255,255,255,0.85)";
    for (const t of [0.35, 0.7]) ctx.fillRect(k.x - H.R * 0.4 + sw, k.y + H.ry * 0.62 * t, H.R * 0.8, H.R * 0.07);
    ctx.restore();
    // Franges.
    for (let i = 0; i < 4; i++) {
      const fx = k.x - H.R * 0.17 + sw * 1.4 + i * H.R * 0.075;
      ctx.beginPath();
      ctx.moveTo(fx, k.y + H.ry * 0.6);
      ctx.lineTo(fx, k.y + H.ry * 0.72);
      ctx.strokeStyle = "#DC2626";
      ctx.lineWidth = H.R * 0.035;
      ctx.lineCap = "round";
      ctx.stroke();
    }
    // Nœud.
    ctx.beginPath();
    ctx.ellipse(k.x, k.y, H.R * 0.17, H.R * 0.14, 0.2, 0, Math.PI * 2);
    ctx.fillStyle = rad(ctx, k.x - H.R * 0.05, k.y - H.R * 0.05, 0, H.R * 0.2, [
      [0, "#F87171"],
      [1, "#B91C1C"],
    ]);
    ctx.fill();
  }
}

export const REAL_OUTFITS: Record<string, RealOutfit> = {
  beanie: { front: beanieFront },
  santaHat: { front: santaFront },
  crown: {
    back: (ctx, H) => crownPart(ctx, H, -1),
    front: (ctx, H, path) => {
      ctx.save();
      ctx.clip(path);
      ctx.clip(capClip(H, crownGeom(H).yb - 0.1, 1));
      clipEvenOdd(ctx, invertClip(H, capClip(H, crownGeom(H).yb, 1)));
      ctx.fillStyle = "rgba(80,50,0,0.12)";
      ctx.fill(path);
      ctx.restore();
      crownPart(ctx, H, 1);
    },
  },
  sunglasses: { frontAfterEyes: true, front: sunglassesFront },
  scarf: { front: scarfFront },
};

// ---------- rendu composite ----------
export function makeHead(W: number, opts: RealOpts): Head {
  const R = W * (opts.scale ?? 1) * 0.3;
  return {
    R,
    rx: R * 1.14,
    ry: R * 0.88,
    view: VIEW_TILT,
    yaw: opts.yaw ?? 0,
    pitch: opts.pitch ?? 0,
    phys: opts.phys ?? { dx: 0, dy: 0 },
  };
}

/** Dessin composite fidèle à la référence : dos -> corps -> yeux -> devant. */
export function drawMochiReal(
  ctx: CanvasRenderingContext2D,
  W: number,
  opts: RealOpts,
): void {
  const H = makeHead(W, opts);
  const o = opts.outfit ? REAL_OUTFITS[opts.outfit] : undefined;
  ctx.save();
  ctx.translate(W / 2, W / 2 + H.R * 0.45);
  if (opts.tilt) ctx.rotate(opts.tilt);
  if (o?.back) o.back(ctx, H);
  const path = drawBody(ctx, H, o?.bodyColors);
  drawEyes(ctx, H, path);
  if (o?.front) o.front(ctx, H, path);
  ctx.restore();
}
