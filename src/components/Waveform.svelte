<script lang="ts">
  import type { RecorderStatus } from "../lib/recorder";

  interface Props {
    /** Mutable level source (0..1). Updated at any rate without re-rendering. */
    level: { current: number };
    status: RecorderStatus;
    /** Target bar count (clamped to available width). */
    bars?: number;
  }

  let { level, status, bars = 120 }: Props = $props();

  let canvas: HTMLCanvasElement | null = $state(null);
  // Statut courant hors tracking : la boucle rAF le lit sans réabonnement.
  let statusBox: RecorderStatus = status;

  $effect(() => {
    statusBox = status;
  });

  const BAR_W = 3;
  const GAP = 4;
  const ATTACK = 0.5;
  const RELEASE = 0.08;

  function readVar(name: string, fallback: string): string {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }

  interface WavePalette {
    idle: string;
    active: string;
    done: string;
    error: string;
  }

  function readPalette(): WavePalette {
    return {
      idle: readVar("--wave-idle", "#9a9a9a"),
      active: readVar("--wave-active", "#ffffff"),
      done: readVar("--green", "#4ade80"),
      error: readVar("--red", "#ef4444"),
    };
  }

  function paletteFor(st: RecorderStatus, p: WavePalette): string {
    switch (st) {
      case "recording":
        return p.active;
      case "processing":
        return p.idle;
      case "done":
        return p.done;
      case "speaking":
        return p.done;
      default:
        return p.idle;
    }
  }

  /** Mélange deux couleurs hex (#rrggbb) : t=0 → a, t=1 → b. */
  function mixHex(a: string, b: string, t: number): string {
    const pa = [1, 3, 5].map((i) => parseInt(a.slice(i, i + 2), 16));
    const pb = [1, 3, 5].map((i) => parseInt(b.slice(i, i + 2), 16));
    const m = pa.map((v, i) => Math.round(v + ((pb[i] ?? v) - v) * Math.max(0, Math.min(1, t))));
    return `#${m.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
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

  $effect(() => {
    const barCount = bars;
    const cv = canvas;
    if (!cv) return;
    const ctx = cv.getContext("2d", { alpha: true });
    if (!ctx) return;
    // Palette lue une fois : getComputedStyle à 60 fps = jank garanti.
    let palette = readPalette();
    const reRead = (): void => {
      palette = readPalette();
    };
    window.addEventListener("whisp:theme", reRead);

    const values: number[] = new Array<number>(barCount).fill(0.04);
    // Morphing de forme : chaque barre a sa vitesse de transition.
    // Quand le statut change, les hauteurs glissent vers la nouvelle forme
    // au lieu de sauter (comme la couleur, mais sur la géométrie).
    const shapeMix: { from: number[]; t0: number; last: RecorderStatus | null } = {
      from: new Array<number>(barCount).fill(0.04),
      t0: -1,
      last: null,
    };
    let raf = 0;
    let width = 0;
    let height = 0;
    let doneFrames = 0;
    // Crossfade couleur : ancien → nouveau sur 300 ms.
    const colorMix = {
      from: paletteFor(statusBox, palette),
      to: paletteFor(statusBox, palette),
      t0: -1,
    };

    const resize = (): void => {
      const dpr = Math.min(2, window.devicePixelRatio || 1);
      const rect = cv.getBoundingClientRect();
      width = Math.max(1, Math.floor(rect.width));
      height = Math.max(1, Math.floor(rect.height));
      cv.width = Math.max(1, Math.floor(rect.width * dpr));
      cv.height = Math.max(1, Math.floor(rect.height * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(cv);

    const render = (now: number): void => {
      const t = now / 1000;
      const st = statusBox;
      // Crossfade UNIVERSEL 350 ms à chaque changement d'état : fini les
      // sauts vert→blanc, blanc→vert, etc. Toutes les transitions sont
      // douces, y compris done→idle (plus de freeze vert, plus de saut).
      const fadeTarget = paletteFor(st, palette);
      if (colorMix.to !== fadeTarget) {
        colorMix.from = mixHex(colorMix.from, colorMix.to, Math.min(1, (t - colorMix.t0) / 0.35));
        colorMix.to = fadeTarget;
        colorMix.t0 = t;
      }
      const kk = colorMix.t0 < 0 ? 1 : Math.min(1, (t - colorMix.t0) / 0.35);
      const color = mixHex(colorMix.from, colorMix.to, kk);
      // Snapshot de forme au changement : mélange ancien → nouveau sur 400 ms.
      if (st !== shapeMix.last) {
        shapeMix.from = values.slice();
        shapeMix.last = st;
        shapeMix.t0 = t;
      }
      const sk = shapeMix.t0 < 0 ? 1 : Math.min(1, (t - shapeMix.t0) / 0.4);
      ctx.clearRect(0, 0, width, height);
      const mid = height / 2;
      // Idle = barres subtiles permanentes (pas de ligne figée).
      // Le shimmer tourne en continu, amplitude mini : vivant mais discret.
      // done : animation de validation en boucle tant que done.
      // error : decay borné puis figé (pas de rAF perpétuel).
      // idle : shimmer CONTINU (jamais figé, sinon pas de respiration).
      const settled = st === "error";
      const step = BAR_W + GAP;
      const count = Math.min(barCount, Math.max(8, Math.floor((width + GAP) / step)));
      const totalW = count * step - GAP;
      let x = (width - totalW) / 2;
      const base = st === "processing" ? 0 : Math.max(0, Math.min(1, level.current));
      let peak = 0;
      for (let i = 0; i < count; i++) {
        let target: number;
        if (st === "processing") {
          // Chargement : vague qui traverse (le texte Transcription… est
          // supprimé, la waveform ELLE-MÊME indique le travail).
          const sweep = ((t * 1.5) % (count + 8)) - 4;
          const d = Math.abs(i - sweep);
          target = 0.06 + 0.3 * Math.exp(-0.4 * d * d);
        } else if (st === "done") {
          // Validation douce : deux vagues sinusoïdales parties des bords
          // qui se rejoignent au centre, période 2,4 s, AUCUN sursaut
          // (pas d'elastic, pas de bounce — tout est en sinus pur).
          const period = 2.4;
          const ph = ((t % period) / period) * Math.PI * 2;
          const c = (count - 1) / 2;
          const half = Math.max(1, c);
          const wavePos = (0.5 - 0.5 * Math.cos(ph)) * (c + 2);
          const left = -2 + wavePos;
          const right = count + 1 - wavePos;
          const dl = (i - left) / Math.max(1, half * 0.4);
          const dr = (i - right) / Math.max(1, half * 0.4);
          target = 0.06 + 0.3 * (Math.exp(-dl * dl) + Math.exp(-dr * dr));
        } else if (st === "speaking") {
          // Réponse agent : vert animé ample (comme si l'agent parlait).
          target = 0.25 + 0.2 * Math.abs(Math.sin(t * 3 + i * 0.7));
        } else if (st === "idle") {
          // Idle vivant : bruissement subtil permanent (petites vagues,
          // amplitude faible). La ligne "respire" au lieu d'être morte.
          const shimmer =
            0.028 + 0.012 * Math.sin(t * 1.3 + i * 0.45) + 0.008 * Math.sin(t * 2.1 + i * 0.9 + 1.7);
          target = Math.max(0.02, shimmer);
        } else {
          const env =
            Math.abs(Math.sin(i * 0.55 + t * 3.1)) *
            (0.35 + 0.65 * Math.abs(Math.sin(i * 0.13 - t * 1.7)));
          // Gain visuel : le RMS voix (~0.1-0.4) remplit la hauteur.
          // sqrt = compression dynamique (faibles audibles, forts saturés doux).
          const boosted = Math.min(1, Math.sqrt(Math.max(0, base) * 2.5));
          target = Math.max(0.05, boosted * (0.35 + 0.65 * env));
        }
        // Forme en arche : gaussienne centrée (haute au milieu, basse aux
        // bords) — la waveform "respire" depuis le centre comme coucou.
        const pos = count > 1 ? (i / (count - 1)) * 2 - 1 : 0;
        const arch = Math.exp(-2.2 * pos * pos);
        target = 0.04 + target * (0.25 + 0.75 * arch);
        // Morphing : la cible est le mélange forme_avant → forme_nouvelle.
        // sk=0 au changement (on part de l'ancienne), sk=1 après 400 ms.
        const prev = shapeMix.from[i] ?? 0.04;
        target = prev + (target - prev) * sk;
        const v = values[i] ?? 0.03;
        const k = target > v ? ATTACK : RELEASE;
        const nv = v + (target - v) * k;
        values[i] = nv;
        peak = Math.max(peak, nv);
        const h = Math.max(3, nv * (height - 8));
        drawBar(ctx, x, mid - h / 2, BAR_W, h, BAR_W / 2, color);
        x += step;
      }
      if (settled) {
        doneFrames += 1;
        // Decay + crossfade : on continue tant que la couleur n'a pas fini
        // sa transition (kk<1) — sinon la ligne reste verte.
        if ((doneFrames < 30 && peak > 0.045) || kk < 1 || sk < 1) {
          raf = requestAnimationFrame(render);
        }
        return;
      }
      doneFrames = 0;
      raf = requestAnimationFrame(render);
    };
    if (statusBox === "idle") {
      render(performance.now());
    } else {
      raf = requestAnimationFrame(render);
    }
    // Changement de statut hors "idle" (ex. recording -> processing) :
    // relancer la boucle si elle s'était arrêtée.
    let lastSeen: RecorderStatus = statusBox;
    const id = window.setInterval(() => {
      // Onglet caché : ne rien relancer (WebKit peint en logiciel sinon,
      // CPU + saccades au scroll sur machine chargée).
      if (typeof document !== "undefined" && document.hidden) return;
      const st = statusBox;
      if (st !== lastSeen) {
        lastSeen = st;
        doneFrames = 0;
        cancelAnimationFrame(raf);
        render(performance.now());
        if (st !== "idle") raf = requestAnimationFrame(render);
      }
    }, 200);

    return () => {
      cancelAnimationFrame(raf);
      window.clearInterval(id);
      window.removeEventListener("whisp:theme", reRead);
      ro.disconnect();
    };
  });
</script>

<canvas bind:this={canvas} class="wave-canvas" aria-hidden="true"></canvas>
