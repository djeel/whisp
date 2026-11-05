<script lang="ts">
  import Toggle from "./Toggle.svelte";
  import type { SettingsStore } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();
</script>

<section class="panel" id="cfg-voice-panel" aria-labelledby="voice-title">
  <div class="card-head">
    <h2 id="voice-title">Voix</h2>
    <p class="card-hint">Voix Fish Audio pour la lecture et les réponses de l'agent. Nécessite la clé Fish + un reference_id (console fish.audio → Text to Speech → voix).</p>
  </div>
  <div class="field-label">
    <span>Voix TTS Fish (reference_id)</span>
    <div class="api-key-row">
      <input
        value={store.voice}
        oninput={(e) => store.pushVoice(e.currentTarget.value)}
        placeholder="Coller le reference_id fish.audio…"
        aria-label="Voix TTS Fish Audio (reference_id)"
        autocomplete="off"
        spellcheck={false}
      />
      <button type="button" class="primary" onclick={() => store.testVoice()} disabled={store.voiceTesting}>
        {store.voiceTesting ? "Lecture…" : "Tester"}
      </button>
    </div>
    {#if store.voiceMsg}
      <span class="field-hint" role="status">{store.voiceMsg}</span>
    {/if}
  </div>
  <div class="check-row">
    <span id="voice-agent">Réponse vocale de l'agent</span>
    <Toggle
      checked={store.agentVoice}
      onchange={(v) => store.pushAgentVoice(v)}
      label="Lire à voix haute les réponses du mode Agent"
    />
  </div>
</section>
