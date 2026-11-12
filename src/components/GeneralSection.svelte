<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import type { DropdownOption } from "./Dropdown.svelte";
  import Toggle from "./Toggle.svelte";
  import type { SettingsStore } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();

  const LANG_OPTS: DropdownOption[] = [
    { value: "fr", label: "Français" },
    { value: "en", label: "English" },
    { value: "auto", label: "Auto (détection)" },
  ];
  // Thème : réglé dans Apparence uniquement (source unique, même store).
  const STT_OPTS: DropdownOption[] = [
    { value: "small", label: "Small — vitesse (défaut)" },
    { value: "base", label: "Base — équilibré" },
    { value: "medium", label: "Medium — qualité" },
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
  const micOpts: DropdownOption[] = $derived([
    { value: "", label: "Système (défaut)", title: "Laissez vide pour le micro système par défaut" },
    ...store.inputDevices.map((d) => ({ value: d, label: d })),
  ]);
</script>

<!-- BEFORE : 7 <select> natifs (figent WebKitGTK) + localStorage dispersé.
     AFTER : Dropdown custom partout, état centralisé dans SettingsStore. -->
<section class="panel" id="cfg-general">
  <div class="card-head">
    <h2>Application</h2>
    <p class="card-hint">Comportement, démarrage et capture.</p>
  </div>
  <div class="check-row">
    <span id="cfg-launch">Lancer au démarrage</span>
    <Toggle checked={store.launchAtLogin} onchange={(v) => store.setLaunchAtLogin(v)} label="Lancer au démarrage" />
  </div>
  {#if store.launchMsg}<p class="card-hint" role="status">{store.launchMsg}</p>{/if}
  <div class="check-row">
    <span id="cfg-sound">Son de fin de transcription</span>
    <Toggle checked={store.prefs.playSound} onchange={(v) => store.patchPrefs({ playSound: v })} label="Son de fin de transcription" />
  </div>
  <div class="check-row">
    <span id="cfg-copy">Coller automatiquement au curseur</span>
    <Toggle
      checked={store.autoPaste}
      onchange={(v) => store.pushAutoPaste(v)}
      label="Coller automatiquement au curseur (Ctrl+V). Désactivé = presse-papiers uniquement."
    />
  </div>
  <div class="check-row">
    <span id="cfg-realtime">Aperçu en temps réel</span>
    <Toggle
      checked={store.realtime}
      onchange={(v) => store.pushRealtime(v)}
      label="Afficher la transcription partielle pendant la dictée"
    />
  </div>
  <div class="check-row">
    <span id="cfg-autostop">Stop auto sur silence</span>
    <Toggle
      checked={store.autoStop}
      onchange={(v) => store.pushAutoStop(v)}
      label="Terminer la dictée après ~1 s de silence (détection vocale)"
    />
  </div>
  <div class="check-row">
    <span>Langue de dictée</span>
    <Dropdown options={LANG_OPTS} bind:value={store.dictLang} label="Langue de dictée" onselect={(v) => store.pushDictLang(v)} />
  </div>
  <div class="check-row">
    <span>Microphone</span>
    <Dropdown
      options={micOpts}
      bind:value={store.inputDevice}
      label="Microphone"
      title="Laissez vide pour le micro système par défaut"
      onselect={(v) => store.pushInputDevice(v)}
    />
  </div>

  <div class="check-row">
    <span>Modèle STT</span>
    <Dropdown options={STT_OPTS} bind:value={store.sttModel} label="Modèle STT" onselect={(v) => store.pushSttModel(v as typeof store.sttModel)} />
  </div>
  <div class="check-row">
    <span>Position de la popup</span>
    <Dropdown
      options={popupOpts}
      bind:value={store.popupPos}
      label="Position de la popup"
      onselect={(v) => store.pushPopupPos(v as typeof store.popupPos)}
    />
  </div>
</section>
