<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import type { SettingsStore, SttKind } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();

  const STT_PER_MODE = [
    { value: "local", label: "Local" },
    { value: "cloud_fish_audio", label: "Fish Audio" },
  ];
  const modesFallback = $derived(
    ["Message", "Mail", "Voice", "Agent"].map((m) => ({
      id: m.toLowerCase(),
      label: m,
      preset: "",
      prompt: "",
      stt: "local",
      llm: "",
      language: "",
    })),
  );
</script>

<section class="panel">
  <div class="card-head">
    <h1>Modes</h1>
    <p class="card-hint">Un moteur voix-texte par mode de dictée.</p>
  </div>
  <p class="muted">
    Voix-texte par mode. Basculez la voix entre Local et Fish Audio
    — {store.hasKey === true ? "clé Fish configurée" : "sans clé Fish, Fish Audio retombe en Local"}.
  </p>
  <ul class="mode-list">
    {#each store.backendModes ?? modesFallback as m (m.id)}
      <li class="mode-row">
        <label class="mode-pick">
          <input
            type="radio"
            name="whisp-mode-tab"
            checked={store.activeMode.toLowerCase() === m.id}
            onchange={() => store.chooseMode(m.label)}
          />
          <span>{m.label}</span>
        </label>
        <Dropdown
          options={STT_PER_MODE}
          value={m.stt === "cloud_fish_audio" ? "cloud_fish_audio" : "local"}
          label={`Voix-texte pour ${m.label}`}
          title={store.hasKey === true ? "Moteur de transcription" : "Ajoutez votre clé Fish dans Configuration pour activer Fish Audio"}
          onselect={(v) => store.setModeStt(m, v as SttKind)}
        />
      </li>
    {/each}
  </ul>
</section>

<style>
  .mode-row :global(.dropdown) {
    flex: 0 1 auto;
    margin-left: auto;
    max-width: 220px;
    min-width: 150px;
  }
</style>
