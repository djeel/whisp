<script lang="ts">
  import type { Approval } from "../lib/recorder";

  interface Props {
    card: Approval;
    onDecide: (id: string, decision: string) => void;
    autoFocus: boolean;
  }

  let { card, onDecide, autoFocus }: Props = $props();

  let secs = $state(60);
  let err: string | null = $state(null);
  let allowBtn: HTMLButtonElement | null = $state(null);

  $effect(() => {
    if (secs <= 0) {
      onDecide(card.request_id, "deny");
      return;
    }
    const id = window.setTimeout(() => {
      secs -= 1;
    }, 1000);
    return () => window.clearTimeout(id);
  });

  // Focus clavier rapide sur Autoriser — seulement hors dictée (jamais en
  // recording : voler le focus casserait la saisie / le flux de dictée).
  $effect(() => {
    if (autoFocus) allowBtn?.focus();
  });

  function click(decision: string): void {
    try {
      onDecide(card.request_id, decision);
    } catch {
      err = "Échec — réessaie via le terminal.";
    }
  }

  const full = $derived(card.command || card.tool);
</script>

<div class="agent-approval" role="group" aria-label="Autorisation {card.agent}">
  <span class="agent-approval-app">{card.agent}</span>
  <pre class="agent-approval-cmd-full" title={full}>{full}</pre>
  <div class="agent-approval-row">
    <span class="agent-approval-timer" role="timer">Expire dans {secs}s</span>
    <span class="agent-spacer"></span>
    <button type="button" class="agent-btn allow" bind:this={allowBtn} onclick={() => click("allow")}>
      Autoriser
    </button>
    <button type="button" class="agent-btn deny" onclick={() => click("deny")}>Refuser</button>
  </div>
  {#if err}<span class="agent-approval-err">{err}</span>{/if}
</div>
