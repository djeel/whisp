<script lang="ts">
  import type { SettingsStore } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();
</script>

<section class="panel">
  <div class="card-head">
    <h1>Vocabulaire</h1>
    <p class="card-hint">Mots et noms propres privilégiés à la transcription.</p>
  </div>
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      store.addWord();
    }}
  >
    <input
      bind:value={store.word}
      placeholder="Ajouter un mot…"
      aria-label="Ajouter un mot au vocabulaire"
    />
    <button type="submit" class="primary">Ajouter</button>
  </form>
  <div class="row">
    <button type="button" class="ghost" onclick={() => store.applyVocabToActiveMode()}>
      Appliquer au mode actif{store.activeModeEntry ? ` (${store.activeModeEntry.label})` : ""}
    </button>
    {#if store.vocabMsg}
      <span class="field-hint" role="status">{store.vocabMsg}</span>
    {/if}
  </div>
  {#if store.vocab.length === 0}
    <div class="empty">
      <svg class="empty-art" width="48" height="32" viewBox="0 0 48 32" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <line x1="6" y1="13" x2="6" y2="19" />
        <line x1="14" y1="9" x2="14" y2="23" />
        <line x1="22" y1="5" x2="22" y2="27" />
        <line x1="30" y1="10" x2="30" y2="22" />
        <line x1="38" y1="14" x2="38" y2="18" />
      </svg>
      <p class="muted">Aucun mot personnalisé pour l'instant.</p>
      <p class="field-hint">Ajoutez un nom propre ci-dessus — il sera privilégié à la transcription.</p>
    </div>
  {:else}
    <ul class="vocab-list">
      {#each store.vocab as w, i (`${w}-${i}`)}
        <li>
          <span>{w}</span>
          <button type="button" class="ghost" onclick={() => store.removeWord(i)}>Retirer</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
