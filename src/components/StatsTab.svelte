<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri";

  interface DictationStats {
    total_entries: number;
    total_words: number;
    today_entries: number;
    avg_words: number;
    minutes_saved: number;
  }

  let { refreshKey = 0 }: { refreshKey?: number } = $props();
  let stats = $state<DictationStats | null>(null);
  let error = $state<string | null>(null);

  let exporting = $state(false);
  function refresh(): void {
    invoke<DictationStats>("dictation_stats")
      .then((s) => {
        stats = s;
        error = null;
      })
      .catch(() => {
        error = "Statistiques indisponibles (backend en cours).";
      });
  }

  async function exportJson(): Promise<void> {
    exporting = true;
    try {
      const entries = await invoke<unknown[]>("list_history", { limit: 1000 });
      const blob = new Blob([JSON.stringify(entries ?? [], null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `whisp-history-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      error = "Export impossible (backend en cours).";
    } finally {
      exporting = false;
    }
  }

  const cards = $derived(
    stats
      ? [
          { label: "Mots dictés", value: String(stats.total_words) },
          { label: "Temps gagné", value: `${Math.round(stats.minutes_saved)} min` },
          { label: "Dictées aujourd'hui", value: String(stats.today_entries) },
          { label: "Mots / dictée", value: stats.avg_words.toFixed(1) },
        ]
      : [],
  );

  onMount(refresh);
  $effect(() => {
    void refreshKey;
    refresh();
  });
</script>

{#if error && !stats}
  <div class="empty">
    <p class="muted">{error}</p>
    <button type="button" class="ghost" onclick={refresh}>Réessayer</button>
  </div>
{:else if !stats}
  <p class="muted" aria-live="polite">Chargement des statistiques…</p>
{:else}
  <div class="stats-grid" role="group" aria-label="Statistiques de dictée">
    {#each cards as c (c.label)}
      <div class="stat-card">
        <span class="stat-value">{c.value}</span>
        <span class="stat-label">{c.label}</span>
      </div>
    {/each}
    <div class="row stats-foot">
      <span class="field-hint">{stats.total_entries} dictée{stats.total_entries > 1 ? "s" : ""} au total (150 mots/min).</span>
      <button type="button" class="ghost" onclick={refresh}>Actualiser</button>
      <button type="button" class="ghost" onclick={exportJson} disabled={exporting}>{exporting ? "Export…" : "Export JSON"}</button>
    </div>
  </div>
  {#if error}
    <p class="field-hint" role="alert">{error} — affichage des dernières données.</p>
  {/if}
{/if}
