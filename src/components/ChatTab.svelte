<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import { CHAT_PROVIDERS, isChatProvider, type ChatProviderId } from "../lib/chatProviders";
  import { invoke } from "../lib/tauri";
  import { readPref, writePref } from "../lib/prefs";

  interface ChatMsg {
    role: "user" | "assistant";
    content: string;
  }

  function loadHistory(): ChatMsg[] {
    try {
      const raw = localStorage.getItem("whisp:chat-history");
      if (!raw) return [];
      const v = JSON.parse(raw) as unknown;
      if (!Array.isArray(v)) return [];
      return v
        .filter(
          (m): m is { role: string; content: string } =>
            typeof m === "object" && m !== null && typeof (m as { content?: unknown }).content === "string",
        )
        .slice(-50)
        .map((m) => ({ role: m.role === "assistant" ? "assistant" : "user", content: m.content }));
    } catch {
      return [];
    }
  }

  const PROVIDER_OPTS = CHAT_PROVIDERS.map((p) => ({ value: p.id, label: p.label }));

  let provider = $state<ChatProviderId>(isChatProvider(readPref("whisp:chat-provider")) ? (readPref("whisp:chat-provider") as ChatProviderId) : "ollama");
  let messages = $state<ChatMsg[]>(loadHistory());
  let draft = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let hasKey = $state<boolean | null>(null);
  let keyInput = $state("");
  let keyHint = $state<string | null>(null);
  let bottom: HTMLDivElement | null = $state(null);

  const needsKey = $derived(provider !== "ollama" && hasKey === false);
  const providerLabel = $derived(PROVIDER_OPTS.find((o) => o.value === provider)?.label ?? provider);
  const canSend = $derived(draft.trim().length > 0 && !busy && !needsKey);

  $effect(() => {
    try {
      localStorage.setItem("whisp:chat-history", JSON.stringify(messages.slice(-50)));
    } catch {
      /* storage unavailable */
    }
    writePref("whisp:chat-provider", provider);
  });
  $effect(() => {
    const p = provider;
    if (p === "ollama") {
      hasKey = null;
      return;
    }
    invoke<boolean>("has_api_key", { provider: p })
      .then((v) => {
        hasKey = v === true;
      })
      .catch(() => {
        hasKey = null;
      });
  });
  $effect(() => {
    void messages.length;
    void busy;
    const id = window.setTimeout(() => {
      bottom?.scrollIntoView({ block: "nearest" });
    }, 150);
    return () => window.clearTimeout(id);
  });

  function saveKey(): void {
    const key = keyInput.trim();
    if (!key) {
      keyHint = provider === "claude" ? "Collez le token `claude setup-token`." : "Collez d'abord votre clé.";
      return;
    }
    invoke("set_api_key", { provider: provider === "claude" ? "claude_oauth" : provider, key })
      .then(() => {
        hasKey = true;
        keyInput = "";
        keyHint = provider === "claude" ? "Compte Claude connecté." : "Clé enregistrée.";
      })
      .catch((e: unknown) => {
        keyHint = e instanceof Error ? e.message : String(e);
      });
  }

  function send(): void {
    const text = draft.trim();
    if (!text || busy) return;
    const next: ChatMsg[] = [...messages, { role: "user" as const, content: text }].slice(-50);
    messages = next;
    draft = "";
    busy = true;
    error = null;
    invoke<string>("chat_command", { messages: next.slice(-20), provider })
      .then((reply) => {
        messages = [...messages, { role: "assistant" as const, content: reply || "…" }].slice(-50);
      })
      .catch((e: unknown) => {
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        busy = false;
      });
  }

  function onComposerKey(e: KeyboardEvent): void {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="chat2">
  <!-- Barre unique : identité + actions -->
  <div class="chat2-top">
    <div class="chat2-id">
      <span class="chat2-dot" aria-hidden="true"></span>
      <div class="chat2-id-text">
        <span class="chat2-name">{providerLabel}</span>
        <span class="chat2-sub">
          {#if provider === "ollama"}
            Local, sans clé
          {:else if hasKey === true}
            {provider === "claude" ? "Compte connecté" : "Clé configurée"}
          {:else if hasKey === false}
            Clé manquante
          {:else}
            Vérification…
          {/if}
        </span>
      </div>
    </div>
    <div class="chat2-actions">
      <Dropdown options={PROVIDER_OPTS} bind:value={provider} label="Fournisseur de chat" />
      <button
        type="button"
        class="ghost"
        onclick={() => {
          messages = [];
          error = null;
        }}
        disabled={messages.length === 0 && !error}
        title="Effacer l'historique local du chat"
      >
        Effacer
      </button>
    </div>
  </div>

  {#if needsKey}
    <div class="chat2-key">
      <input
        type="password"
        bind:value={keyInput}
        placeholder={provider === "claude" ? "claude setup-token → sk-ant-oat-…" : `Coller la clé ${provider}…`}
        aria-label={provider === "claude" ? "Token abonnement (claude setup-token)" : `Clé API ${provider}`}
        autocomplete="off"
        spellcheck={false}
        onkeydown={(e) => {
          if (e.key === "Enter") saveKey();
        }}
      />
      <button type="button" class="primary" onclick={saveKey}>
        {provider === "claude" ? "Connecter" : "Enregistrer"}
      </button>
    </div>
    {#if keyHint}<p class="field-hint" role="status">{keyHint}</p>{/if}
  {/if}

  <!-- Zone centrale : vide hero OU conversation -->
  <div class="chat2-body" role="log" aria-label="Conversation">
    {#if messages.length === 0 && !busy}
      <div class="chat2-hero">
        <span class="chat2-dot big" aria-hidden="true"></span>
        <p class="chat2-hero-title">Discutez avec {providerLabel}</p>
        <p class="muted">Posez une question pour démarrer la conversation.</p>
      </div>
    {:else}
      <div class="chat2-list">
        {#each messages as m, i (i)}
          <div class="chat2-msg {m.role}">
            {#if m.role === "assistant"}<span class="chat2-who">Assistant</span>{/if}
            <p>{m.content}</p>
          </div>
        {/each}
        {#if busy}
          <div class="chat2-msg assistant pending" aria-live="polite">
            <span class="chat2-who">Assistant</span>
            <p>
              <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
              Réponse en cours…
            </p>
          </div>
        {/if}
        <div bind:this={bottom}></div>
      </div>
    {/if}
  </div>

  {#if error}
    <p class="field-hint chat2-error" role="alert">{error}</p>
  {/if}

  <!-- Composer ancré en bas -->
  <form
    class="chat2-input"
    onsubmit={(e) => {
      e.preventDefault();
      send();
    }}
  >
    <input
      bind:value={draft}
      placeholder={needsKey ? "Ajoutez votre clé ci-dessus pour discuter…" : "Écrivez votre message… (Entrée pour envoyer)"}
      aria-label="Message à envoyer"
      disabled={busy || needsKey}
      onkeydown={onComposerKey}
    />
    <button type="submit" class="primary" disabled={!canSend}>
      {busy ? "…" : "Envoyer"}
    </button>
  </form>
</div>

<style>
  .chat2 {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 480px;
  }
  /* Barre identité + actions */
  .chat2-top {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .chat2-id {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 160px;
  }
  .chat2-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
  }
  .chat2-id-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .chat2-name {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chat2-sub {
    font-size: 12px;
    color: var(--muted);
  }
  .chat2-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .chat2-actions :global(.dropdown) {
    flex: 0 1 220px;
    min-width: 140px;
  }
  /* Clé API */
  .chat2-key {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .chat2-key input {
    flex: 1;
    min-width: 0;
  }
  /* Corps : hero OU liste scrollable */
  .chat2-body {
    flex: 1;
    min-height: 200px;
    display: flex;
    flex-direction: column;
  }
  .chat2-hero {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 4px;
    padding: 32px 16px;
  }
  .chat2-dot.big {
    width: 12px;
    height: 12px;
    margin-bottom: 8px;
  }
  .chat2-hero-title {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .chat2-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 420px;
    overflow-y: auto;
    padding: 4px 2px;
    scroll-behavior: smooth;
  }
  .chat2-msg {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 14px;
    border-radius: 16px;
    max-width: 85%;
    line-height: 1.5;
  }
  .chat2-msg.user {
    align-self: flex-end;
    background: var(--accent);
    color: var(--accent-text);
    border-bottom-right-radius: 6px;
  }
  .chat2-msg.assistant {
    align-self: flex-start;
    background: var(--bg-input);
    border: 1px solid var(--line);
    border-bottom-left-radius: 6px;
  }
  .chat2-msg p {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .chat2-msg.pending p {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chat2-who {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .chat2-msg.user .chat2-who {
    display: none;
  }
  .chat2-error {
    margin: 0;
  }
  /* Composer */
  .chat2-input {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .chat2-input input {
    flex: 1;
    min-width: 0;
  }
</style>
