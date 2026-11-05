<script lang="ts">
  import { invoke } from "../lib/tauri";
  import { formatHistoryDate, type SettingsStore } from "../lib/settings.svelte";
  import StatsTab from "./StatsTab.svelte";
  import type { FlowTab } from "../routes/Settings.svelte";

  let { store, goConfig }: { store: SettingsStore; goConfig: (t: FlowTab) => void } = $props();

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
  <p class="hints">
    <span>Stop <kbd>Alt + Espace</kbd></span>
    <span>Cancel <kbd>Échap</kbd></span>
  </p>
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
      <StatsTab refreshKey={store.refreshTick} />
    </section>
    <section class="panel agent-teaser">
      <h2>The Notetaker</h2>
      <p class="card-hint">Votre agent lancé avec la dictée comme demande.</p>
      <button type="button" class="ghost teaser-link" onclick={() => goConfig("config")}>Ouvrir l'agent →</button>
    </section>
  </div>
</div>
<section class="panel history">
  <div class="history-head">
    <div class="card-head">
      <h2>Historique</h2>
      <p class="card-hint">Vos dernières dictées sur cet appareil.</p>
    </div>
  </div>
  {#if store.history.length === 0}
    <div class="empty">
      <svg class="empty-art" width="48" height="32" viewBox="0 0 48 32" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <circle cx="24" cy="16" r="9" />
        <line x1="24" y1="12" x2="24" y2="16" />
        <line x1="24" y1="16" x2="27" y2="18" />
      </svg>
      <p class="muted">Aucune transcription pour l'instant.</p>
      <p class="field-hint">Lancez un enregistrement depuis l'onglet Home.</p>
    </div>
  {:else}
    <ul class="history-list">
      {#each store.history as h (h.id)}
        <li>
          <p>{h.text_final || h.transcript_raw}</p>
          <div class="meta">
            <span>{h.mode}</span>
            <span>{formatHistoryDate(h.timestamp)}</span>
            <button
              type="button"
              class="ghost"
              onclick={() => store.deleteEntry(h.id)}
              aria-label={`Supprimer cette dictée (${h.mode})`}
            >
              Supprimer
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .teaser-link {
    align-self: flex-start;
  }
</style>
