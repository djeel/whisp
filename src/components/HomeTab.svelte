<script lang="ts">
  import { invoke } from "../lib/tauri";
  import type { SettingsStore } from "../lib/settings.svelte";
  import StatsTab from "./StatsTab.svelte";

  let { store }: { store: SettingsStore } = $props();

  function startRecording(): void {
    invoke("start_recording").catch(() => {
      /* backend en cours */
    });
  }
</script>

<section class="panel hero">
  <div class="card-head">
    <h1>Dictée vocale</h1>
    <p class="card-hint">Parlez n'importe où, le texte est collé au curseur. Historique local sur cet appareil.</p>
  </div>
  <div class="row hero-actions">
    <button type="button" class="primary" onclick={startRecording}>
      Démarrer l'enregistrement
    </button>
  </div>
  <p class="hints">
    <span>Stop <kbd>Alt + Espace</kbd></span>
    <span>Cancel <kbd>Échap</kbd></span>
  </p>
</section>

<section class="panel">
  <div class="card-head">
    <h2>Activité</h2>
    <p class="card-hint">Votre activité de dictée, calculée en local.</p>
  </div>
  <StatsTab refreshKey={store.refreshTick} />
</section>

<style>
  .hero-actions {
    margin-top: 12px;
  }
  .hints {
    display: flex;
    gap: 18px;
    margin: 12px 0 0;
    font-size: 13px;
    color: var(--muted);
    flex-wrap: wrap;
  }
</style>
