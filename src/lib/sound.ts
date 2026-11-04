/** Tiny WebAudio beeps for the recorder. No audio files. */

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  try {
    if (!ctx) {
      const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!AC) return null;
      ctx = new AC();
    }
    if (ctx.state === "suspended") void ctx.resume().catch(() => undefined);
    return ctx;
  } catch {
    return null;
  }
}

function beep(freq: number, delay: number, dur: number, gain = 0.12): void {
  const ac = audio();
  if (!ac) return;
  try {
    const t = ac.currentTime + delay;
    const osc = ac.createOscillator();
    const g = ac.createGain();
    osc.type = "sine";
    osc.frequency.value = freq;
    g.gain.setValueAtTime(0, t);
    g.gain.linearRampToValueAtTime(gain, t + 0.01);
    g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
    osc.connect(g).connect(ac.destination);
    osc.start(t);
    osc.stop(t + dur + 0.05);
  } catch {
    /* audio unavailable */
  }
}

function enabled(): boolean {
  try {
    const raw = localStorage.getItem("whisp:prefs");
    if (!raw) return true;
    const prefs = JSON.parse(raw) as { playSound?: boolean };
    return prefs.playSound !== false;
  } catch {
    return true;
  }
}

export type Cue = "start" | "stop" | "finish" | "error";

/** Play one UI cue, unless `playSound` is off in Configuration. */
export function playCue(cue: Cue): void {
  if (!enabled()) return;
  switch (cue) {
    case "start":
      beep(440, 0, 0.12);
      break;
    case "stop":
      beep(660, 0, 0.12);
      break;
    case "finish":
      beep(660, 0, 0.1);
      beep(880, 0.1, 0.16);
      break;
    case "error":
      beep(180, 0, 0.25, 0.16);
      break;
  }
}
