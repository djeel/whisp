<script lang="ts">
  import Dropdown from "../components/Dropdown.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { CHAT_PROVIDERS, isChatProvider } from "../lib/chatProviders";
  import type { SettingsStore } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();

  const PROVIDER_OPTS = CHAT_PROVIDERS.map((p) => ({ value: p.id, label: p.label }));
  const needsBaseUrl = $derived(["deepseek", "qwen", "moonshot", "custom", "openrouter"].includes(store.chatProvider));
</script>

<section class="panel" id="cfg-pills">
  <div class="card-head">
    <h2>Active pills</h2>
    <p class="card-hint">Choisissez ce que whisp affiche — {store.activePills.length}/{store.pillDefs.length} affichées.</p>
  </div>
  {#each store.pillDefs as p (p.id)}
    <div class="check-row">
      <span>{p.label}<span class="field-hint"> — {p.hint}</span></span>
      <Toggle checked={store.activePills.includes(p.id)} onchange={() => store.togglePill(p.id)} label={p.label} />
    </div>
  {/each}
</section>

<section class="panel" id="cfg-claude">
  <div class="card-head">
    <h2>Claude (abonnement)</h2>
    <p class="card-hint">Connectez votre compte Claude (Pro/Max, sans clé API) + votre Style — utilisés par le chat et le raffinage.</p>
  </div>
  <div class="field-label">
    <span>Token abonnement{store.chatKeyState === true && store.chatProvider === "claude" ? " (connecté)" : ""}</span>
    <div class="api-key-row">
      <input
        type="password"
        bind:value={store.claudeToken}
        placeholder="claude setup-token → sk-ant-oat-…"
        aria-label="Token abonnement Claude (claude setup-token)"
        autocomplete="off"
        spellcheck={false}
      />
      <button type="button" class="primary" onclick={() => store.saveClaudeToken()}>Connecter</button>
    </div>
    {#if store.claudeHint}
      <span class="field-hint">{store.claudeHint}</span>
    {:else}
      <span class="field-hint">Terminal : `claude setup-token`, collez le token ici. Stocké côté backend (600), jamais affiché.</span>
    {/if}
  </div>
  <div class="field-label">
    <span>Style Claude (ton préréglé)</span>
    <textarea
      value={store.claudeStyle}
      oninput={(e) => store.pushClaudeStyle(e.currentTarget.value)}
      placeholder="Coller le texte du Style (claude.ai → Styles → Edit)…"
      aria-label="Style Claude (system prompt)"
      rows={3}
      spellcheck={false}
    ></textarea>
    {#if store.claudeStyleHint}
      <span class="field-hint">{store.claudeStyleHint}</span>
    {:else}
      <span class="field-hint">Injecté en `system` pour Claude + Anthropic (chat, voix, raffinage).</span>
    {/if}
  </div>
</section>

<section class="panel" id="cfg-chat">
  <div class="card-head">
    <h2>Chat providers</h2>
    <p class="card-hint">Discutez via Claude (abonnement), Anthropic, OpenAI ou un endpoint OpenAI-compatible.</p>
  </div>
  <div class="check-row">
    <span>Fournisseur</span>
    <Dropdown
      options={PROVIDER_OPTS}
      value={store.chatProvider}
      label="Fournisseur"
      onselect={(v) => {
        if (isChatProvider(v)) store.pushChatProvider(v);
      }}
    />
  </div>
  <label class="check-row">
    <span>Modèle</span>
    <input
      value={store.chatModel}
      oninput={(e) => store.pushChatModel(e.currentTarget.value)}
      placeholder={store.chatProvider === "ollama"
        ? "whisp-nanbeige"
        : store.chatProvider === "claude"
          ? "claude-haiku-4-5"
          : "ex. deepseek-chat, gemini-2.0-flash, qwen-plus"}
      aria-label="Modèle du fournisseur de chat"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  {#if needsBaseUrl}
    <label class="check-row">
      <span>Base URL (OpenAI-compatible)</span>
      <input
        value={store.chatBaseUrl}
        oninput={(e) => store.pushChatBaseUrl(e.currentTarget.value)}
        placeholder="https://…/v1 (vide = défaut du provider)"
        aria-label="Base URL OpenAI-compatible"
        autocomplete="off"
        spellcheck={false}
      />
    </label>
  {/if}
  {#if store.chatProvider === "claude"}
    <div class="field-label">
      <span>Token abonnement{store.chatKeyState === true ? " (connecté)" : store.chatKeyState === false ? " (absent)" : ""}</span>
      <div class="api-key-row">
        <input
          type="password"
          bind:value={store.chatKey}
          placeholder="claude setup-token → sk-ant-oat-…"
          aria-label="Token abonnement Claude"
          autocomplete="off"
          spellcheck={false}
        />
        <button type="button" class="primary" onclick={() => store.saveChatKey()}>Connecter</button>
      </div>
      {#if store.chatKeyHint}<span class="field-hint">{store.chatKeyHint}</span>{/if}
    </div>
  {:else if store.chatProvider !== "ollama"}
    <div class="field-label">
      <span>Clé {store.chatProvider}{store.chatKeyState === true ? " (configurée)" : store.chatKeyState === false ? " (absente)" : ""}</span>
      <div class="api-key-row">
        <input
          type="password"
          bind:value={store.chatKey}
          placeholder={`Coller la clé ${store.chatProvider}…`}
          aria-label={`Clé ${store.chatProvider}`}
          autocomplete="off"
          spellcheck={false}
        />
        <button type="button" class="primary" onclick={() => store.saveChatKey()}>Enregistrer</button>
      </div>
      {#if store.chatKeyHint}<span class="field-hint">{store.chatKeyHint}</span>{/if}
    </div>
  {:else}
    <p class="field-hint">Ollama local : aucune clé requise (voir Modèles locaux).</p>
  {/if}
</section>

<section class="panel">
  <div class="card-head">
    <h2>Modèles locaux</h2>
    <p class="card-hint">Ollama sur cet ordinateur — rien ne quitte votre PC.</p>
  </div>
  <label class="check-row">
    <span>Ollama URL</span>
    <input
      value={store.ollamaUrl}
      oninput={(e) => store.pushOllamaUrl(e.currentTarget.value)}
      placeholder="http://127.0.0.1:11434"
      aria-label="Adresse du serveur Ollama"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  <label class="check-row">
    <span>Ollama modèle</span>
    <input
      value={store.ollamaModel}
      oninput={(e) => store.pushOllamaModel(e.currentTarget.value)}
      placeholder="whisp-nanbeige"
      aria-label="Modèle Ollama"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  <div class="row">
    <button type="button" class="ghost" onclick={() => store.testOllama()} disabled={store.ollamaBusy} aria-busy={store.ollamaBusy}>
      {store.ollamaBusy ? "Test…" : "Tester"}
    </button>
    {#if store.ollamaMsg}<span class="field-hint" role="status">{store.ollamaMsg}</span>{/if}
  </div>
</section>

<style>
  .check-row :global(.dropdown) {
    flex: 0 1 auto;
    margin-left: auto;
    max-width: 55%;
    min-width: 180px;
  }
</style>
