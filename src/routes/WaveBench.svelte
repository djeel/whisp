<script lang="ts">
  import { onMount } from "svelte";
  import Waveform from "../components/Waveform.svelte";
  import type { RecorderStatus } from "../lib/recorder";

  /** Banc d'essai visuel : tous les états de la waveform + boucle forcée.
   * Route dev : ?window=wavebench — usage interne, validation animations. */

  const STATES: RecorderStatus[] = ["idle", "recording", "processing", "done", "error", "speaking"];

  let status = $state<RecorderStatus>("recording");
  let auto = $state(true);
  let log = $state<string[]>([]);
  const level = { current: 0.5 };

  onMount(() => {
    const lvlId = window.setInterval(() => {
      level.current = 0.35 + 0.3 * Math.abs(Math.sin(performance.now() / 900));
    }, 50);
    return () => window.clearInterval(lvlId);
  });

  $effect(() => {
    if (!auto) return;
    let i = STATES.indexOf(status);
    const id = window.setInterval(() => {
      i = (i + 1) % STATES.length;
      const next = STATES[i] ?? "idle";
      status = next;
      log = [...log.slice(-11), `${new Date().toLocaleTimeString()} → ${next}`];
    }, 2500);
    return () => window.clearInterval(id);
  });
</script>

<div class="bench">
  <h1 class="bench-title">Waveform — banc d'essai</h1>
  <p class="bench-hint">
    Auto-boucle les 6 états (2,5 s chacun). Vérifie : arche centrée, crossfade sans saut, pas de freeze, boucle rAF active seulement hors idle.
  </p>
  <div class="bench-row">
    {#each STATES as s}
      <button
        type="button"
        class="bench-btn"
        class:selected={status === s}
        onclick={() => {
          auto = false;
          status = s;
        }}
        aria-pressed={status === s}
      >
        {s}
      </button>
    {/each}
    <button type="button" class="bench-btn" onclick={() => (auto = !auto)}>
      {auto ? "⏸ pause" : "▶ auto"}
    </button>
  </div>
  <div class="bench-card">
    <Waveform {level} {status} />
    <p class="bench-state">État actuel : <strong>{status}</strong></p>
  </div>
  <pre class="bench-log">{log.join("\n") || "(transitions loggées ici)"}</pre>
</div>

<style>
  .bench {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    background: #141414;
    min-height: 100vh;
    color: #e8e8e8;
  }
  .bench-title {
    margin: 0;
    font-size: 20px;
  }
  .bench-hint {
    margin: 0;
    font-size: 13px;
    opacity: 0.7;
  }
  .bench-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .bench-btn {
    padding: 6px 12px;
    border-radius: 8px;
    border: 1px solid #555;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .bench-btn.selected {
    border: 2px solid #4ade80;
  }
  .bench-card {
    border: 1px solid #333;
    border-radius: 16px;
    padding: 20px;
    background: #1e1e1e;
    max-width: 720px;
  }
  .bench-state {
    margin: 8px 0 0;
    font-size: 12px;
    opacity: 0.7;
  }
  .bench-log {
    font-size: 12px;
    opacity: 0.8;
    margin: 0;
    max-height: 200px;
    overflow-y: auto;
  }
</style>
