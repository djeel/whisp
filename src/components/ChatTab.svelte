<script lang="ts">
  import { onMount } from "svelte";
  import Dropdown from "./Dropdown.svelte";
  import { CHAT_PROVIDERS, isChatProvider, type ChatProviderId } from "../lib/chatProviders";
  import { listen, invoke } from "../lib/tauri";
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
  const keyLabel = $derived(provider === "claude" ? "Token abonnement (`claude setup-token`)" : `Clé API ${provider}`);
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
  // Autoscroll en bas à chaque message (throttlé anti layout-thrash).
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

  onMount(() => {
    void listen("whisp://noop-chat", () => undefined).catch(() => undefined);
  });
</script>

<div class="chat">
  <div class="chat-head">
    <div class="chat-id">
      <span class="chat-model-dot" aria-hidden="true"></span>
      <div class="chat-id-text">
        <span class="chat-id-name">{providerLabel}</span>
        <span class="chat-id-sub">
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
    <div class="chat-head-actions">
      <Dropdown options={PROVIDER_OPTS} bind:value={provider} label="Fournisseur de chat" />
      <button
        type="button"
        class="ghost"
        onclick={() => {
          messages = [];
        }}
        disabled={messages.length === 0 && !error}
        title="Effacer l'historique local du chat"
      >
        Effacer
      </button>
    </div>
  </div>

  {#if needsKey}
    <div class="chat-key">
      <input
        type="password"
        bind:value={keyInput}
        placeholder={provider === "claude" ? "claude setup-token → sk-ant-oat-…" : `Coller la clé ${provider}…`}
        aria-label={keyLabel}
        autocomplete="off"
        spellcheck={false}
      />
      <button type="button" class="primary" onclick={saveKey}>
        {provider === "claude" ? "Connecter" : "Enregistrer"}
      </button>
    </div>
    {#if keyHint}<p class="field-hint" role="status">{keyHint}</p>{/if}
  {/if}

  <div class="chat-log" role="log" aria-label="Conversation" class:empty-log={messages.length === 0 && !busy}>
    {#if messages.length === 0 && !busy}
      <div class="chat-empty">
        <p class="chat-empty-title">Discutez avec {providerLabel}</p>
        <p class="muted">Posez une question pour démarrer la conversation.</p>
      </div>
    {:else}
      {#each messages as m, i (i)}
        <div class="chat-msg {m.role}">
          {#if m.role === "assistant"}<span class="chat-who">Assistant</span>{/if}
          <p>{m.content}</p>
        </div>
      {/each}
    {/if}
    {#if busy}
      <div class="chat-msg assistant pending" aria-live="polite">
        <span class="chat-who">Assistant</span>
        <p>
          <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
          Réponse en cours…
        </p>
      </div>
    {/if}
    <div bind:this={bottom}></div>
  </div>

  {#if error}
    <p class="field-hint chat-error" role="alert">{error}</p>
  {/if}

  <form
    class="chat-input"
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
  .chat {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  /* En-tête : identité modèle + actions. */
  .chat-head {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .chat-id {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 180px;
  }
  .chat-model-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
  }
  .chat-id-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .chat-id-name {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chat-id-sub {
    font-size: 12px;
    color: var(--muted);
  }
  .chat-head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .chat-head-actions :global(.dropdown) {
    flex: 0 1 220px;
    min-width: 150px;
  }
  /* Clé API : ligne compacte. */
  .chat-key {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .chat-key input {
    flex: 1;
    min-width: 0;
  }
  /* Log : bulles user à droite, assistant à gauche. */
  .chat-log {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .chat-log.empty-log {
    min-height: 160px;
    padding: 28px 16px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--muted);
    border-radius: var(--radius-panel);
  }
  .chat-empty {
    text-align: center;
  }
  .chat-empty-title {
    margin: 0 0 4px;
    font-size: 15px;
    font-weight: 600;
  }
  .chat-error {
    margin: 0;
  }
  /* Composer : champ + bouton même hauteur. */
  .chat-input {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .chat-input input {
    flex: 1;
    min-width: 0;
  }
</style>
