<script lang="ts">
  import { invoke } from "../lib/tauri";
  import StatsTab from "./StatsTab.svelte";
  import type { FlowTab } from "../routes/Settings.svelte";

  let { refreshTick = 0, goConfig }: { refreshTick?: number; goConfig: (t: FlowTab) => void } = $props();

  function startRecording(): void {
    invoke("start_recording").catch(() => {
      /* backend en cours */
    });
  }
</script>

<section class="panel page-head">
  <h1>Hello</h1>
  <p class="card-hint">Maintenez Alt+Espace n'importe où, parlez, relâchez : c'est collé.</p>
  <div class="row">
    <button type="button" class="primary" onclick={startRecording}>
      <span aria-hidden="true">● </span>Démarrer l'enregistrement
    </button>
  </div>
</section>
<div class="home-grid">
  <section class="panel">
    <div class="card-head">
      <h2>Dictée vocale</h2>
      <p class="card-hint">Parlez n'importe où, le texte est collé au curseur. Historique local sur cet appareil.</p>
    </div>
    <div class="empty">
      <p class="muted">Alt+Espace pour dicter, Échap pour annuler.</p>
    </div>
  </section>
  <div class="home-side">
    <section class="panel home-stats">
      <div class="card-head">
        <h2>Activité</h2>
        <p class="card-hint">Votre activité de dictée, calculée en local.</p>
      </div>
      <StatsTab refreshKey={refreshTick} />
    </section>
    <section class="panel agent-teaser">
      <h2>The Notetaker</h2>
      <p class="card-hint">Votre agent lancé avec la dictée comme demande.</p>
      <button type="button" class="ghost teaser-link" onclick={() => goConfig("config")}>Ouvrir l'agent →</button>
    </section>
  </div>
</div>

<style>
  .teaser-link {
    align-self: flex-start;
  }
</style>
