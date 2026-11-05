<script module lang="ts">
  export type FlowTab = "home" | "stats" | "chat" | "modes" | "vocabulary" | "config";
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import AppearanceSection from "../components/AppearanceSection.svelte";
  import ChatTab from "../components/ChatTab.svelte";
  import GeneralSection from "../components/GeneralSection.svelte";
  import HomeTab from "../components/HomeTab.svelte";
  import ModesTab from "../components/ModesTab.svelte";
  import ProvidersSection from "../components/ProvidersSection.svelte";
  import ShortcutsSection from "../components/ShortcutsSection.svelte";
  import StatsTab from "../components/StatsTab.svelte";
  import UpdatesSection from "../components/UpdatesSection.svelte";
  import VocabTab from "../components/VocabTab.svelte";
  import VoiceSection from "../components/VoiceSection.svelte";
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
    tab =
      raw === "stats" || raw === "chat" || raw === "modes" || raw === "vocabulary" || raw === "config"
        ? raw
        : "home";
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
      <HomeTab {store} />
    {:else if tab === "stats"}
      <section class="panel">
        <div class="card-head">
          <h1>Statistiques</h1>
          <p class="card-hint">Votre activité de dictée, calculée en local.</p>
        </div>
        <StatsTab />
      </section>
    {:else if tab === "chat"}
      <section class="panel">
        <div class="card-head">
          <h1>Chat</h1>
          <p class="card-hint">Discutez avec votre modèle local ou cloud.</p>
        </div>
        <ChatTab />
      </section>
    {:else if tab === "modes"}
      <ModesTab {store} />
    {:else if tab === "vocabulary"}
      <VocabTab {store} />
    {:else}
      <section class="panel page-head">
        <h1>Settings</h1>
        <p class="card-hint">Comportement, voix, agents et apparence.</p>
      </section>
      <GeneralSection {store} />
      <ProvidersSection {store} />
      <div id="cfg-voice"><VoiceSection {store} /></div>
      <div id="cfg-shortcuts"><ShortcutsSection /></div>
      <AppearanceSection {store} />
      <UpdatesSection />
    {/if}
  </main>
</div>
