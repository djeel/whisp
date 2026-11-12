<script lang="ts">
  import Button from "./Button.svelte";
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri";

  interface HistoryEntry {
    id: string;
    text_final: string;
    mode?: string;
    created_at?: string;
  }

  let { } = $props();
  let entries = $state<HistoryEntry[]>([]);
  let busy = $state<string | null>(null);
  let notice = $state<string | null>(null);

  async function refresh(): Promise<void> {
    try {
      const list = await invoke<HistoryEntry[]>("list_history", { limit: 10 });
      entries = list ?? [];
    } catch {
      entries = [];
    }
  }

  async function repaste(id: string): Promise<void> {
    busy = id;
    notice = null;
    try {
      const msg = await invoke<string>("repaste", { id });
      notice = typeof msg === "string" ? msg : "Re-collé.";
    } catch (e) {
      notice = e instanceof Error ? e.message : "Re-collage impossible (backend en cours).";
    } finally {
      busy = null;
    }
  }

  async function undoPaste(): Promise<void> {
    busy = "undo";
    notice = null;
    try {
      const msg = await invoke<string>("undo_paste");
      notice = typeof msg === "string" ? msg : "Collage annulé.";
    } catch (e) {
      notice = e instanceof Error ? e.message : "Rien à annuler.";
    } finally {
      busy = null;
    }
  }

  onMount(refresh);
</script>

<section class="panel">
  <div class="card-head">
    <h2>Historique récent</h2>
    <p class="card-hint">Re-collez une dictée au curseur, ou annulez le dernier collage.</p>
  </div>
  <div class="row">
    <Button onclick={undoPaste} disabled={busy === "undo"} busy={busy === "undo"}>Annuler le dernier collage</Button>
  </div>
  {#if entries.length === 0}
    <p class="muted">Aucune dictée pour l'instant.</p>
  {:else}
    <ul class="recent">
      {#each entries as e (e.id)}
        <li>
          <span class="recent-text" title={e.text_final}>{e.text_final}</span>
          <Button onclick={() => repaste(e.id)} disabled={busy === e.id} busy={busy === e.id}>Re-coller</Button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if notice}
    <p class="field-hint" role="status">{notice}</p>
  {/if}
</section>

<style>
  .recent {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .recent li {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .recent-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
</style>
