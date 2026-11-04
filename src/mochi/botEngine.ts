// BotEngine — port fidèle du moteur d'animation coucou
// (/tmp/coucou-study/windows/src/mochi/engine.ts, classe BotEngine).
// Seule l'ANIMATION est portée ici (timings, springs, tweens, emotes) :
// le DESSIN reste dans realEngine.ts (corps/yeux/tenues).
// Pas de son, pas de mailbox/morph, pas de particules : hors scope whisp.

export type BotEyeShape =
  | "pill" | "wide" | "happy" | "closed" | "flat" | "wink" | "dot";

export type BotEmoteName =
  | "love" | "surprised" | "proud" | "wink" | "yawn" | "happy" | "annoyed";

export type WhispMood =
  | "idle" | "thinking" | "working" | "finished" | "error" | "approval";

const Ease = {
  out: (t: number) => 1 - Math.pow(1 - t, 3),
  inOut: (t: number) =>
    t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2,
  back: (t: number) => {
    const c = 1.70158;
    return 1 + (c + 1) * Math.pow(t - 1, 3) + c * Math.pow(t - 1, 2);
  },
  lin: (t: number) => t,
};

type EaseFn = (t: number) => number;
type TweenKey = readonly [target: number, durationMs: number, ease: EaseFn];

interface Tween {
  prop: TweenProp;
  keys: TweenKey[];
  index: number;
  from: number;
  startMs: number;
  onComplete?: () => void;
}

type TweenProp =
  | "yaw" | "pitch" | "tilt" | "open" | "sx" | "sy"
  | "oy" | "ox" | "es" | "blush" | "hands" | "outfitPresence";

const now = (): number => performance.now() / 1000;

const EMOTE_EYE: Record<BotEmoteName, BotEyeShape> = {
  love: "happy",
  surprised: "dot",
  proud: "happy",
  wink: "wink",
  yawn: "closed",
  happy: "happy",
  annoyed: "flat",
};

const STATE_EYE: Record<WhispMood, BotEyeShape> = {
  idle: "pill",
  thinking: "pill",
  working: "pill",
  finished: "happy",
  error: "flat",
  approval: "wide",
};

export class BotEngine {
  // État animé (le `s` de coucou).
  yaw = 0;
  pitch = 0;
  tilt = 0;
  open = 1;
  sx = 1;
  sy = 1;
  oy = 0;
  ox = 0;
  es = 1;
  blush = 0;
  hands = 0;

  // Ressort des parties molles (pompons, bouts d'écharpe), −1…1.
  physDx = 0;
  physDy = 0;
  private physVx = 0;
  private physVy = 0;
  private prevYaw = 0;
  private prevOy = 0;

  // Cibles (lissage exponentiel par frame).
  private tgYaw = 0;
  private tgPitch = 0;
  private tgTilt = 0;
  private tgSy = 1;
  private tgSx = 1;
  private tgEs = 1;

  // Tenue : `outfit` = ce qui est dessiné, présence 0 = absente.
  // Changement animé : sortie 180 ms, entrée 350 ms + squash.
  outfit = "none";
  outfitPresence = 0;
  private outfitTarget = "none";

  eyeOverride: BotEyeShape | null = null;
  private eyeOverrideUntil = 0;
  private permanentEye: BotEyeShape | null = null;
  permanentEmote: BotEmoteName | null = null;

  private tweens = new Map<TweenProp, Tween>();
  private locks = new Set<TweenProp>();

  lookX = 0;
  lookY = 0;

  mood: WhispMood = "idle";

  private t0 = now() - Math.random() * 5;
  private nextBlink = now() + 1.5 + Math.random() * 2;
  private greetToken = 0;
  waveUntil = 0;
  private waveStart = 0;

  // ── API publique ──────────────────────────────────────────────────

  /** Mapping whisp → états coucou : idle→idle, recording→thinking(scans), etc. */
  setMood(next: WhispMood): void {
    if (this.mood === next) return;
    this.mood = next;
    switch (next) {
      case "finished":
        this.greet();
        break;
      case "error":
        this.anim("ox", [
          [0.08, 50, Ease.out],
          [-0.08, 70, Ease.inOut],
          [0.05, 70, Ease.inOut],
          [0, 90, Ease.out],
        ]);
        break;
      case "approval":
        this.anim("oy", [[-0.2, 150, Ease.out], [0, 300, Ease.back]]);
        break;
      default:
        this.blink();
    }
  }

  blink(): void {
    if (this.locks.has("open")) return;
    this.anim("open", [
      [0.06, 70, Ease.inOut],
      [1, 130, Ease.out],
    ]);
  }

  squash(): void {
    this.physVy += 0.6;
    this.anim("sy", [
      [0.78, 70, Ease.out],
      [1.1, 130, Ease.out],
      [1, 170, Ease.inOut],
    ]);
    this.anim("sx", [
      [1.16, 70, Ease.out],
      [0.95, 130, Ease.out],
      [1, 170, Ease.inOut],
    ]);
  }

  /** La joie de coucou : yeux happy 2 s + petit saut + clignements. */
  greet(): void {
    const t = now();
    const tok = ++this.greetToken;
    this.waveStart = t + 0.45;
    this.waveUntil = t + 1.55;
    this.physVx += 0.2;
    this.eyeOverride = "happy";
    this.eyeOverrideUntil = t + 2.0;
    this.anim("oy", [[-0.06, 220, Ease.out], [0.0, 220, Ease.back]]);
    setTimeout(() => {
      if (this.greetToken !== tok) return;
      this.anim("sy", [[0.95, 100, Ease.out], [1.0, 260, Ease.back]]);
      this.anim("sx", [[1.04, 100, Ease.out], [1.0, 260, Ease.back]]);
    }, 250);
    setTimeout(() => {
      if (this.greetToken === tok) this.blink();
    }, 550);
    setTimeout(() => {
      if (this.greetToken === tok) this.blink();
    }, 1500);
  }

  setPermanentEmote(emote: BotEmoteName | null): void {
    this.permanentEmote = emote;
    this.permanentEye = emote ? EMOTE_EYE[emote] : null;
    if (this.permanentEye) {
      this.eyeOverride = this.permanentEye;
      this.eyeOverrideUntil = Number.POSITIVE_INFINITY;
    } else if (this.eyeOverrideUntil === Number.POSITIVE_INFINITY) {
      this.eyeOverride = null;
      this.eyeOverrideUntil = 0;
    }
  }

  triggerEmote(emote: BotEmoteName, duration = 1.8): void {
    const t = now();
    this.eyeOverride = EMOTE_EYE[emote];
    this.eyeOverrideUntil = t + duration;
    switch (emote) {
      case "love":
        this.anim("blush", [
          [1, 300, Ease.out],
          [1, (duration - 0.6) * 1000, Ease.lin],
          [0, 300, Ease.inOut],
        ]);
        this.anim("oy", [[-0.1, 160, Ease.out], [0, 300, Ease.back]]);
        break;
      case "surprised":
        this.anim("oy", [[-0.3, 140, Ease.out], [0, 380, Ease.back]]);
        this.anim("es", [[1.25, 120, Ease.out], [1, 500, Ease.inOut]]);
        break;
      case "proud":
        this.anim("blush", [
          [0.7, 250, Ease.out],
          [0.7, (duration - 0.5) * 1000, Ease.lin],
          [0, 300, Ease.inOut],
        ]);
        break;
      case "wink":
        this.anim("tilt", [
          [0.12, 160, Ease.out],
          [0.12, (duration - 0.4) * 1000, Ease.lin],
          [0, 240, Ease.inOut],
        ]);
        break;
      case "yawn":
        this.anim("sy", [[1.12, 500, Ease.inOut], [1, 500, Ease.inOut]]);
        this.anim("sx", [[0.94, 500, Ease.inOut], [1, 500, Ease.inOut]]);
        break;
      case "happy":
        this.anim("blush", [[0.6, 200, Ease.out], [0, 600, Ease.inOut]]);
        break;
      case "annoyed":
        this.eyeOverride = "flat";
        this.eyeOverrideUntil = t + 0.8;
        break;
    }
  }

  setOutfit(next: string, animated = true): void {
    if (next === this.outfitTarget) return;
    this.outfitTarget = next;
    this.tweens.delete("outfitPresence");
    this.locks.delete("outfitPresence");
    const enter = (): void => {
      this.outfit = next;
      this.anim("outfitPresence", [[1, 350, Ease.inOut]], () => this.squash());
    };
    if (!animated) {
      this.outfit = next;
      this.outfitPresence = next !== "none" ? 1 : 0;
    } else if (next === "none") {
      this.anim("outfitPresence", [[0, 180, Ease.inOut]], () => {
        this.outfit = "none";
      });
    } else if (this.outfit === "none") {
      this.outfitPresence = 0;
      enter();
    } else {
      this.anim("outfitPresence", [[0, 180, Ease.inOut]], enter);
    }
  }

  get eye(): BotEyeShape {
    return this.eyeOverride ?? this.permanentEye ?? STATE_EYE[this.mood];
  }

  get seconds(): number {
    return now() - this.t0;
  }

  anim(prop: TweenProp, keys: TweenKey[], onComplete?: () => void): void {
    this.tweens.set(prop, {
      prop,
      keys,
      index: 0,
      from: this[prop],
      startMs: performance.now(),
      onComplete,
    });
    this.locks.add(prop);
  }

  // ── Update par frame ──────────────────────────────────────────────

  update(dt: number): void {
    const n = now();
    const nowMs = performance.now();

    for (const tw of [...this.tweens.values()]) {
      const k = tw.keys[tw.index];
      const p = Math.min(1, Math.max(0, (nowMs - tw.startMs) / k[1]));
      this[tw.prop] = tw.from + (k[0] - tw.from) * k[2](p);
      if (p >= 1) {
        tw.from = k[0];
        tw.index += 1;
        tw.startMs = nowMs;
        if (tw.index >= tw.keys.length) {
          this.tweens.delete(tw.prop);
          this.locks.delete(tw.prop);
          tw.onComplete?.();
        }
      }
    }

    const t = n - this.t0;
    let ty = this.lookX * 0.62;
    let tp = this.lookY * 0.5;

    // thinking = scans coucou : regard qui balaie.
    if (this.mood === "thinking") {
      ty = Math.sin(t * 2.6) * 0.6;
      tp = -0.06;
    }

    this.tgYaw = ty;
    this.tgPitch = tp;
    this.tgTilt = this.mood === "finished" ? 0 : 0;

    if (n > this.waveStart && n < this.waveUntil) {
      const wt = n - this.waveStart;
      this.tgTilt = -0.06 + Math.sin(2 * Math.PI * 1.2 * wt) * 0.07;
    }

    // working = bounces coucou : rebond −|sin(t·5.2)|·0.07.
    const bounce = this.mood === "working" ? -Math.abs(Math.sin(t * 5.2)) * 0.07 : 0;
    const kGen = 1 - Math.pow(0.0008, dt);
    if (!this.locks.has("oy")) this.oy += (bounce - this.oy) * kGen;

    // idle (+finished) = breathes coucou : 1 + sin(t·1.8)·0.035.
    if (this.mood === "idle" || this.mood === "finished") {
      const amp = 0.035;
      this.tgSy = 1 + Math.sin(t * 1.8) * amp;
      this.tgSx = 1 - Math.sin(t * 1.8) * amp * 0.57;
    } else if (this.mood === "working") {
      this.tgSy = 1;
      this.tgSx = 1;
    } else {
      this.tgSy = 1;
      this.tgSx = 1;
    }

    const kLook = 1 - Math.pow(0.0025, dt);
    if (!this.locks.has("yaw")) this.yaw += (this.tgYaw - this.yaw) * kLook;
    if (!this.locks.has("pitch")) this.pitch += (this.tgPitch - this.pitch) * kLook;
    if (!this.locks.has("tilt")) this.tilt += (this.tgTilt - this.tilt) * kGen;
    if (!this.locks.has("sy")) this.sy += (this.tgSy - this.sy) * kGen;
    if (!this.locks.has("sx")) this.sx += (this.tgSx - this.sx) * kGen;
    if (!this.locks.has("es")) this.es += (this.tgEs - this.es) * kGen;

    // Clignement : 1.5 + rand·2 s (mêmes bornes que coucou à l'init).
    if (n > this.nextBlink) {
      this.blink();
      if (Math.random() < 0.22) setTimeout(() => this.blink(), 230);
      this.nextBlink = n + 1.5 + Math.random() * 2;
    }

    if (this.eyeOverride && n > this.eyeOverrideUntil) {
      this.eyeOverride = this.permanentEye;
      if (this.permanentEye) this.eyeOverrideUntil = Number.POSITIVE_INFINITY;
      else this.eyeOverrideUntil = 0;
    }

    // Ressort parties molles : raideur 60, amortissement 9 (coucou).
    if (dt > 0) {
      const yawVel = (this.yaw - this.prevYaw) / dt;
      const oyVel = (this.oy - this.prevOy) / dt;
      const tDx = Math.max(-1, Math.min(1, -yawVel * 0.35 - this.tilt * 2));
      const tDy = Math.max(-1, Math.min(1, oyVel * 0.5));
      this.physVx += (60 * (tDx - this.physDx) - 9 * this.physVx) * dt;
      this.physVy += (60 * (tDy - this.physDy) - 9 * this.physVy) * dt;
      this.physDx += this.physVx * dt;
      this.physDy += this.physVy * dt;
    }
    this.prevYaw = this.yaw;
    this.prevOy = this.oy;
  }
}
