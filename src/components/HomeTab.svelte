<script lang="ts">
  import Button from "./Button.svelte";
  import Dot from "./Dot.svelte";
  import { invoke } from "../lib/tauri";
  import type { SettingsStore, TabId } from "../lib/settings.svelte";
  import StatsTab from "./StatsTab.svelte";
  import ModesTab from "./ModesTab.svelte";
  import VocabTab from "./VocabTab.svelte";
  import HistoryTab from "./HistoryTab.svelte";
  import HealthTab from "./HealthTab.svelte";

  let { store, goTab }: { store: SettingsStore; goTab: (t: TabId) => void } = $props();

  function goAgents(): void {
    goTab("agents");
  }

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
  {#if store.setupState === "no-llm"}
    <div class="setup-banner" role="status">
      <Dot tone="done" pulse />
      <div class="setup-text">
        <strong>Voix + raffinage non configurés.</strong>
        <span>Sans Ollama ni clé Fish, la dictée colle le texte brut (sans ponctuation ni mise en forme).</span>
      </div>
      <div class="setup-actions">
        <Button variant="primary" onclick={() => goAgents()}>Configurer</Button>
      </div>
    </div>
  {/if}
  <div class="row hero-actions">
    <Button variant="primary" onclick={startRecording}>
      Démarrer l'enregistrement
    </Button>
  </div>
  <p class="hints">
    <span>Stop <kbd>Alt + Espace</kbd></span>
    <span>Cancel <kbd>Échap</kbd></span>
  </p>
</section>

<ModesTab {store} />
<VocabTab {store} />
<HistoryTab />
<HealthTab />
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
  .setup-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--accent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .setup-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
    flex: 1;
    min-width: 0;
  }
  .setup-text span {
    color: var(--muted);
  }
  .setup-actions {
    flex-shrink: 0;
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
