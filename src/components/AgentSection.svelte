<script lang="ts">
  import Button from "./Button.svelte";
  import Dot from "./Dot.svelte";
  import Dropdown from "./Dropdown.svelte";
  import type { DropdownOption } from "./Dropdown.svelte";
  import Toggle from "./Toggle.svelte";
  import { truncate120, type SettingsStore } from "../lib/settings.svelte";
  import { CHAT_PROVIDERS, isChatProvider } from "../lib/chatProviders";

  let { store }: { store: SettingsStore } = $props();
  const AGENT_OPTS: DropdownOption[] = [
    { value: "omp", label: "omp (défaut)" },
    { value: "claude", label: "Claude Code" },
    { value: "codex", label: "Codex" },
    { value: "custom", label: "Personnalisé…" },
  ];
  const PROVIDER_OPTS = CHAT_PROVIDERS.map((p) => ({ value: p.id, label: p.label }));
  const needsBaseUrl = $derived(["deepseek", "qwen", "moonshot", "custom", "openrouter"].includes(store.chatProvider));
</script>

<section class="panel" id="cfg-agent">
  <div class="card-head">
    <h2>Agent</h2>
    <p class="card-hint">Agent lancé avec votre dictée comme demande.</p>
  </div>
  <div class="check-row">
    <span>Agent</span>
    <Dropdown
      options={AGENT_OPTS}
      bind:value={store.agentName}
      label="Agent"
      title="Agent lancé par le mode Agent avec ta dictée comme demande"
      onselect={(v) => store.pushAgentName(v)}
    />
  </div>
  <p class="field-hint">Binaire repli : omp si l'agent choisi est absent.</p>
  {#if store.agentName === "custom"}
    <label class="check-row">
      <span>Commande agent</span>
      <input
        value={store.agentCommand}
        oninput={(e) => store.pushAgentCommand(e.currentTarget.value)}
        placeholder="ma-cli --prompt {prompt}"
        aria-label="Commande agent personnalisée, {prompt} = ta dictée"
      />
    </label>
  {/if}
  {#if store.agentStatus}
    <section class="agent-card" aria-label="Session agent">
      <div class="agent-head">
        <h2 class="agent-title">Session agent</h2>
        <Dot badge tone={store.agentStatus.active ? "working" : "idle"}>
          {store.agentStatus.active ? "Active" : "Aucune"}
        </Dot>
      </div>
      <dl class="agent-rows">
        <div class="agent-row">
          <dt>Agent</dt>
          <dd>{store.agentStatus.agent}</dd>
        </div>
        <div class="agent-row">
          <dt>Dossier</dt>
          <dd title={store.agentStatus.cwd}>{store.agentStatus.cwd}</dd>
        </div>
        <div class="agent-row">
          <dt>Dernier prompt</dt>
          <dd title={store.agentStatus.lastPrompt ?? undefined}>
            {store.agentStatus.lastPrompt ? truncate120(store.agentStatus.lastPrompt) : "—"}
          </dd>
        </div>
      </dl>
      <div class="row agent-actions">
        <Button
          variant="primary"
          onclick={() => store.openAgentTerminal()}
          disabled={store.agentLaunching}
          busy={store.agentLaunching}
        >
          {store.agentLaunching ? "Lancement…" : "Ouvrir le terminal"}
        </Button>
        <Button onclick={() => store.refreshAgentStatus()}>Actualiser</Button>
      </div>
      {#if store.agentOpenMsg}<p class="field-hint" role="status">{store.agentOpenMsg}</p>{/if}
    </section>
  {/if}
</section>

<section class="panel" id="cfg-agents">
  <div class="card-head">
    <h2>Agents suivis</h2>
    <p class="card-hint">Suivi des sessions via hooks (popup agent) — un statut par agent, installation via install-hook.sh.</p>
  </div>
  {#each store.agentHooks ?? [] as a (a.id)}
    <div class="check-row">
      <span>
        <Dot tone={a.installed ? "working" : "blocked"} />
        {a.name}
        <span class="field-hint"> — {a.installed ? "installé" : "non installé"} · {a.path}</span>
      </span>
      <Button
        variant="primary"
        disabled={store.agentBusy === a.id}
        busy={store.agentBusy === a.id}
        onclick={() => store.installAgent(a.id)}
      >
        {store.agentBusy === a.id ? "Installation…" : a.installed ? "Réinstaller" : "Installer"}
      </Button>
    </div>
  {/each}
  {#if store.agentHooks === null}<p class="field-hint">Statut hooks indisponible (backend en cours).</p>{/if}
  {#if store.agentMsg}<p class="field-hint" role="status">{store.agentMsg}</p>{/if}
</section>

<section class="panel" id="cfg-pills">
  <div class="card-head">
    <h2>Affichage</h2>
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
    <p class="card-hint">Connectez votre compte Claude (Pro/Max, sans clé API) + votre Style — utilisés par le raffinage.</p>
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
      <Button variant="primary" onclick={() => store.saveClaudeToken()}>Connecter</Button>
    </div>
    {#if store.claudeHint}
      <span class="field-hint">{store.claudeHint}</span>
    {:else}
      <span class="field-hint">Terminal : `claude setup-token`, collez le token ici. Stocké côté backend (600), jamais affiché.</span>
    {/if}
  </div>
  <div class="field-label">
    <span>Style Claude (ton prédéfini)</span>
    <textarea
      bind:value={store.claudeStyle}
      placeholder="Coller le texte du Style (claude.ai → Styles → Edit)…"
      aria-label="Style Claude (system prompt)"
      rows={3}
      spellcheck={false}
      oninput={() => store.pushClaudeStyle(store.claudeStyle)}
    ></textarea>
    {#if store.claudeStyleHint}
      <span class="field-hint">{store.claudeStyleHint}</span>
    {:else}
      <span class="field-hint">Injecté en `system` pour Claude + Anthropic (voix, raffinage).</span>
    {/if}
  </div>
</section>

<section class="panel" id="cfg-models">
  <div class="card-head">
    <h2>Modèles</h2>
    <p class="card-hint">Fournisseur de raffinage et modèles locaux.</p>
  </div>
  <label class="check-row">
    <span>Fournisseur</span>
    <Dropdown
      options={PROVIDER_OPTS}
      value={store.chatProvider}
      label="Fournisseur de raffinage"
      onselect={(v) => {
        if (isChatProvider(v)) store.pushChatProvider(v);
      }}
    />
  </label>
  <label class="check-row">
    <span>Modèle</span>
    <input
      bind:value={store.chatModel}
      oninput={() => store.pushChatModel(store.chatModel)}
      placeholder={store.chatProvider === "ollama" ? "whisp-nanbeige" : store.chatProvider === "claude" ? "claude-haiku-4-5" : "ex. deepseek-chat"}
      aria-label="Modèle du fournisseur"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  {#if needsBaseUrl}
    <label class="check-row">
      <span>Base URL</span>
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
          value={store.chatKey}
        oninput={(e) => { store.chatKey = e.currentTarget.value; }}
          placeholder="claude setup-token → sk-ant-oat-…"
          aria-label="Token abonnement Claude"
          autocomplete="off"
          spellcheck={false}
        />
        <Button variant="primary" onclick={() => store.saveChatKey()}>Connecter</Button>
      </div>
      {#if store.chatKeyHint}<span class="field-hint">{store.chatKeyHint}</span>{/if}
    </div>
  {:else if store.chatProvider !== "ollama"}
    <div class="field-label">
      <span>Clé {store.chatProvider}{store.chatKeyState === true ? " (configurée)" : store.chatKeyState === false ? " (absente)" : ""}</span>
      <div class="api-key-row">
        <input
          type="password"
          value={store.chatKey}
        oninput={(e) => { store.chatKey = e.currentTarget.value; }}
          placeholder={`Coller la clé ${store.chatProvider}…`}
          aria-label={`Clé ${store.chatProvider}`}
          autocomplete="off"
          spellcheck={false}
        />
        <Button variant="primary" onclick={() => store.saveChatKey()}>Enregistrer</Button>
      </div>
      {#if store.chatKeyHint}<span class="field-hint">{store.chatKeyHint}</span>{/if}
    </div>
  {:else}
    <p class="field-hint">Ollama local : aucune clé requise (voir Modèles locaux).</p>
  {/if}
  <div class="card-head" style="margin-top: 8px">
    <h2>Modèles locaux</h2>
    <p class="card-hint">Ollama sur cet ordinateur — rien ne quitte votre PC.</p>
  </div>
  <label class="check-row">
    <span>Ollama URL</span>
    <input
      bind:value={store.ollamaUrl}
      oninput={() => store.pushOllamaUrl(store.ollamaUrl)}
      placeholder="http://127.0.0.1:11434"
      aria-label="Adresse du serveur Ollama"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  <label class="check-row">
    <span>Ollama modèle</span>
    <input
      bind:value={store.ollamaModel}
      oninput={() => store.pushOllamaModel(store.ollamaModel)}
      placeholder="whisp-nanbeige"
      aria-label="Modèle Ollama"
      autocomplete="off"
      spellcheck={false}
    />
  </label>
  <div class="row">
    <Button onclick={() => store.testOllama()} disabled={store.ollamaBusy} busy={store.ollamaBusy}>
      {store.ollamaBusy ? "Test…" : "Tester"}
    </Button>
    {#if store.ollamaMsg}<span class="field-hint" role="status">{store.ollamaMsg}</span>{/if}
  </div>
</section>

<section class="panel" id="cfg-fish">
  <div class="card-head">
    <h2>Voix Fish Audio</h2>
    <p class="card-hint">Clé + voix pour la transcription cloud et la lecture.</p>
  </div>
  <div class="field-label">
    <span>Clé Fish Audio{store.hasKey === true ? " (configurée)" : store.hasKey === false ? " (absente)" : ""}</span>
    <div class="api-key-row">
      <input
        type="password"
        value={store.apiKey}
        oninput={(e) => { store.apiKey = e.currentTarget.value; }}
        placeholder="Coller la clé Fish Audio…"
        aria-label="Clé API Fish Audio"
        autocomplete="off"
        spellcheck={false}
      />
      <Button variant="primary" onclick={() => store.saveApiKey()}>Enregistrer</Button>
      <Button onclick={() => store.testApiKey()}>Tester</Button>
    </div>
    {#if store.apiHint}<span class="field-hint">{store.apiHint}</span>{/if}
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
      <Button variant="primary" onclick={() => store.testVoice()} disabled={store.voiceTesting} busy={store.voiceTesting}>
        {store.voiceTesting ? "Lecture…" : "Tester"}
      </Button>
    </div>
    {#if store.voiceMsg}<span class="field-hint" role="status">{store.voiceMsg}</span>{/if}
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
