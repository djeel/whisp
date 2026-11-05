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

  // BEFORE : 3 useEffect séparés (persist, writePref, has_api_key).
  // AFTER : 2 $effect (persist+writePref fusionnés) + $effect provider→clé.
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
  // Autoscroll throttlé (anti layout-thrash, cf. Chat.tsx).
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

  onMount(() => {
    void listen("whisp://noop-chat", () => undefined).catch(() => undefined);
  });
</script>

<div class="chat">
  <div class="chat-bar">
    <span class="chat-provider-label">Fournisseur</span>
    <Dropdown options={PROVIDER_OPTS} bind:value={provider} label="Fournisseur de chat" />
    {#if provider === "claude"}
      <span class="field-hint">{hasKey === true ? "Compte Claude connecté." : hasKey === false ? "Compte non connecté." : "État de la connexion inconnu."}</span>
    {:else if provider !== "ollama"}
      <span class="field-hint">{hasKey === true ? "Clé configurée." : hasKey === false ? "Clé manquante." : "État de la clé inconnu."}</span>
    {:else}
      <span class="field-hint">Local, sans clé (whisp-nanbeige).</span>
    {/if}
    <span class="agent-spacer"></span>
    <button
      type="button"
      class="ghost"
      onclick={() => {
        messages = [];
      }}
      disabled={messages.length === 0}
      title="Effacer l'historique local du chat"
    >
      Effacer
    </button>
  </div>
  {#if needsKey}
    <div class="row chat-key">
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
      {#if keyHint}<span class="field-hint">{keyHint}</span>{/if}
    </div>
  {/if}
  <div class="chat-log" role="log" aria-label="Conversation" class:empty-log={messages.length === 0 && !busy}>
    {#if messages.length === 0 && !busy}
      <p class="muted">Posez une question pour démarrer la conversation.</p>
    {:else}
      {#each messages as m, i (i)}
        <div class="chat-msg {m.role}">
          <span class="chat-who">{m.role === "user" ? "Vous" : "Assistant"}</span>
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
    <p class="field-hint" role="alert">{error}</p>
  {/if}
  <form
    class="row chat-input"
    onsubmit={(e) => {
      e.preventDefault();
      send();
    }}
  >
    <input
      bind:value={draft}
      placeholder="Écrivez votre message…"
      aria-label="Message à envoyer"
      disabled={busy}
    />
    <button type="submit" class="primary" disabled={busy || !draft.trim()}>
      {busy ? "…" : "Envoyer"}
    </button>
  </form>
</div>

<style>
  .chat-bar {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .chat-provider-label {
    font-size: 14px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chat-bar :global(.dropdown) {
    flex: 0 1 240px;
    min-width: 160px;
    margin-left: auto;
  }
  .chat-log.empty-log {
    min-height: 120px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--line);
    border-radius: var(--radius-panel);
  }
  /* Champ + bouton alignés sur la même ligne de base. */
  .chat-input {
    align-items: center;
  }
</style>
