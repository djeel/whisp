<script module lang="ts">
  export type FlowTab = "home" | "stats" | "app" | "apparence" | "agents";
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import AppearanceSection from "../components/AppearanceSection.svelte";
  import AgentSection from "../components/AgentSection.svelte";
  import GeneralSection from "../components/GeneralSection.svelte";
  import ShortcutsSection from "../components/ShortcutsSection.svelte";
  import HomeTab from "../components/HomeTab.svelte";
  import StatsTab from "../components/StatsTab.svelte";
  import UpdatesSection from "../components/UpdatesSection.svelte";
  import { SettingsStore, FLOW_TABS, type TabId } from "../lib/settings.svelte";
  import { applyAppearance } from "../lib/accent";
  import { useConfigFlushOnDestroy } from "../lib/tauri";
  import { writePref } from "../lib/prefs";

  useConfigFlushOnDestroy();
  const store = new SettingsStore();
  let tab = $state<TabId>("home");

  onMount(() => {
    const raw = (() => {
      try {
        return localStorage.getItem("whisp:tab");
      } catch {
        return null;
      }
    })();
    tab = raw === "stats" || raw === "app" || raw === "apparence" || raw === "agents" ? (raw as TabId) : raw === "config" ? "app" : "home";
    void store.init();
  });
  onDestroy(() => {
    store.destroy();
  });

  // BEFORE : useEffect[theme] écrivait applyAppearance+writePref à chaque render.
  // AFTER : $effect theme — snapshot $state, pas de ref.
  $effect(() => {
    const t = store.theme;
    applyAppearance(t === "system" ? null : t, store.accentSource, store.accentCustom, store.caelestia);
    writePref("whisp:theme", t);
  });

  function go(next: TabId): void {
    tab = next;
    store.goTab(next);
  }
</script>

<div class="settings">
  <aside class="side">
    <div class="brand">
      <span>whisp</span>
    </div>
    <nav class="side-nav" aria-label="Sections">
      <div class="tabs" role="tablist" aria-label="Onglets">
        {#each FLOW_TABS as t (t.id)}
          <button
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            class="tab"
            class:active={tab === t.id}
            onclick={() => go(t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </nav>

  </aside>

  <main class="content">
    {#if tab === "home"}
      <HomeTab {store} goTab={go} />
    {:else if tab === "stats"}
      <section class="panel">
        <div class="card-head">
          <h1>Statistiques</h1>
          <p class="card-hint">Votre activité de dictée, calculée en local.</p>
        </div>
        <StatsTab />
      </section>
    {:else if tab === "app"}
      <section class="panel page-head">
        <h1>Application</h1>
        <p class="card-hint">Comportement, démarrage et capture.</p>
      </section>
      <GeneralSection {store} />
      <div id="cfg-shortcuts"><ShortcutsSection /></div>
      <UpdatesSection />
    {:else if tab === "apparence"}
      <section class="panel page-head">
        <h1>Apparence</h1>
        <p class="card-hint">Thème, popup et carte d'enregistrement.</p>
      </section>
      <AppearanceSection {store} />
    {:else}
      <section class="panel page-head">
        <h1>Agents & Voix</h1>
        <p class="card-hint">Agents, modèles IA et voix Fish Audio.</p>
      </section>
      <AgentSection {store} />
    {/if}
  </main>
</div>
