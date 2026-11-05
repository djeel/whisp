<script lang="ts">
  import { onDestroy } from "svelte";
  import Dropdown from "./Dropdown.svelte";
  import type { DropdownOption } from "./Dropdown.svelte";
  import {
    applyAccent,
    applyAppearance,
    isAccentHex,
    type AccentSource,
  } from "../lib/accent";
  import { useConfigFlushOnDestroy } from "../lib/tauri";
  import type { PopupSize, SettingsStore } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();
  useConfigFlushOnDestroy();

  const THEME_OPTS: DropdownOption[] = [
    { value: "system", label: "Système" },
    { value: "dark", label: "Sombre" },
    { value: "light", label: "Clair" },
  ];
  const ACCENT_OPTS: DropdownOption[] = [
    { value: "theme", label: "Thème (défaut)" },
    { value: "custom", label: "Personnalisée" },
    { value: "caelestia", label: "Caelestia (fond d'écran)" },
  ];
  const SIZE_OPTS: DropdownOption[] = [
    { value: "S", label: "S — 512px" },
    { value: "M", label: "M — 576px" },
    { value: "L", label: "L — 704px" },
  ];

  const popupOpts: DropdownOption[] = $derived(
    (["top-center", "center", "bottom-center", "cursor"] as const).map((id) => ({
      value: id,
      label:
        id === "top-center"
          ? "Haut-centre (Superwhisper)"
          : id === "center"
            ? "Centre"
            : id === "bottom-center"
              ? "Bas-centre"
              : "Près du curseur",
    })),
  );

  // BEFORE : AppearanceSection lisait themeRef.current (closure stale évitée via ref).
  // AFTER : $effect lit store.theme direct — le snapshot $state remplace la ref.
  $effect(() => {
    const t = store.theme;
    applyAppearance(t === "system" ? null : t, store.accentSource, store.accentCustom, store.caelestia);
  });
  $effect(() => {
    if (store.accentSource === "caelestia" || store.accentSource === "custom") {
      applyAccent(store.accentSource, store.accentCustom, store.caelestia);
    } else {
      applyAccent("theme", store.accentCustom, store.caelestia);
    }
  });

  onDestroy(() => {
    store.flushConfig();
  });
</script>

<section class="panel">
  <div class="card-head">
    <h2>Apparence</h2>
    <p class="card-hint">Thème, popup et carte d'enregistrement.</p>
  </div>
  <div class="check-row">
    <span>Thème</span>
    <Dropdown options={THEME_OPTS} bind:value={store.theme} label="Thème" onselect={(v) => store.pushTheme(v as typeof store.theme)} />
  </div>
  <div class="check-row">
    <span>Couleur du recorder</span>
    <Dropdown
      options={ACCENT_OPTS}
      value={store.accentSource}
      label="Couleur du recorder"
      onselect={(v) => store.pushAccentSource(v as AccentSource)}
    />
  </div>
  {#if store.accentSource === "custom"}
    <label class="check-row">
      <span>Couleur personnalisée</span>
      <span class="color-row">
        <input
          type="color"
          value={isAccentHex(store.accentCustom) ? store.accentCustom : "#f59e0b"}
          oninput={(e) => store.pushAccentCustom(e.currentTarget.value)}
          aria-label="Couleur d'accent du recorder"
        />
        <code>{store.accentCustom}</code>
      </span>
    </label>
  {/if}
  {#if store.accentSource === "caelestia"}
    <p class="field-hint" role="status">
      {store.caelestia
        ? `Caelestia : ${store.caelestia.primary} (${store.caelestia.mode}, suit le fond d'écran).`
        : "Caelestia introuvable — repli sur le thème (installez Caelestia ou choisissez une autre source)."}
    </p>
  {/if}
  <div class="check-row">
    <span>Position de la popup</span>
    <Dropdown
      options={popupOpts}
      bind:value={store.popupPos}
      label="Position de la popup"
      onselect={(v) => store.pushPopupPos(v as typeof store.popupPos)}
    />
  </div>
  <div class="check-row">
    <span>Taille de la popup</span>
    <Dropdown
      options={SIZE_OPTS}
      value={store.size}
      label="Taille de la popup"
      onselect={(v) => store.pushSize(v as PopupSize)}
    />
  </div>
  <label class="check-row">
    <span>Opacité de la carte ({Math.round(store.opacity * 100)} %)</span>
    <input
      type="range"
      min={0.4}
      max={1}
      step={0.05}
      value={store.opacity}
      oninput={(e) => store.pushOpacity(Number(e.currentTarget.value))}
      aria-label="Opacité de la carte popup"
    />
  </label>
  {#if store.sizeMsg}
    <span class="field-hint" role="status">{store.sizeMsg}</span>
  {/if}
</section>

<style>
  .check-row :global(.dropdown) {
    flex: 0 1 auto;
    margin-left: auto;
    max-width: 55%;
    min-width: 180px;
  }
  .color-row {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    margin-left: auto;
  }
  .color-row input[type="color"] {
    width: 44px;
    height: 28px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg-input);
    cursor: pointer;
  }
  .color-row code {
    font-size: 13px;
    font-family: ui-monospace, monospace;
    color: var(--text);
    min-width: 64px;
  }
</style>
