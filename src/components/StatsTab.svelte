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

{#if error}
  <p class="field-hint">{error}</p>
{:else if !stats}
  <p class="muted">Chargement des statistiques…</p>
{:else}
  <div class="stats-grid" role="group" aria-label="Statistiques de dictée">
    {#each cards as c (c.label)}
      <div class="stat-card">
        <span class="stat-value">{c.value}</span>
        <span class="stat-label">{c.label}</span>
      </div>
    {/each}
    <div class="row stats-foot">
      <span class="field-hint">{stats.total_entries} dictées au total (150 mots/min).</span>
      <button type="button" class="ghost" onclick={refresh}>Actualiser</button>
    </div>
  </div>
{/if}
