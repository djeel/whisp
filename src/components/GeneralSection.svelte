<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import type { DropdownOption } from "./Dropdown.svelte";
  import Toggle from "./Toggle.svelte";
  import { truncate120, type SettingsStore, type SttKind } from "../lib/settings.svelte";

  let { store }: { store: SettingsStore } = $props();

  const LANG_OPTS: DropdownOption[] = [
    { value: "fr", label: "Français" },
    { value: "en", label: "English" },
    { value: "auto", label: "Auto (détection)" },
  ];
  const THEME_OPTS: DropdownOption[] = [
    { value: "system", label: "Système" },
    { value: "dark", label: "Sombre" },
    { value: "light", label: "Clair" },
  ];
  const STT_OPTS: DropdownOption[] = [
    { value: "small", label: "Small — vitesse (défaut)" },
    { value: "base", label: "Base — équilibré" },
    { value: "medium", label: "Medium — qualité" },
  ];
  const AGENT_OPTS: DropdownOption[] = [
    { value: "omp", label: "omp (défaut)" },
    { value: "claude", label: "Claude Code" },
    { value: "codex", label: "Codex" },
    { value: "custom", label: "Personnalisé…" },
  ];
  const STT_PER_MODE: Array<{ value: SttKind; label: string }> = [
    { value: "local", label: "Local" },
    { value: "cloud_fish_audio", label: "Fish Audio" },
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
  const modesFallback = $derived(
    ["Message", "Mail", "Voice", "Agent"].map((m) => ({
      id: m.toLowerCase(),
      label: m,
      preset: "",
      prompt: "",
      stt: "local",
      llm: "",
      language: "",
    })),
  );
</script>

<!-- BEFORE : 7 <select> natifs (figent WebKitGTK) + localStorage dispersé.
     AFTER : Dropdown custom partout, état centralisé dans SettingsStore. -->
<section class="panel" id="cfg-general">
  <div class="card-head">
    <h2>General</h2>
    <p class="card-hint">Comportement général de l'application.</p>
  </div>
  <div class="check-row">
    <span id="cfg-launch">Lancer au démarrage</span>
    <Toggle checked={store.prefs.launchAtLogin} onchange={(v) => store.patchPrefs({ launchAtLogin: v })} label="Lancer au démarrage" />
  </div>
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
    <span>Thème</span>
    <Dropdown options={THEME_OPTS} bind:value={store.theme} label="Thème" onselect={(v) => store.pushTheme(v as typeof store.theme)} />
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
  <div class="field-label">
    <span>
      Clé Fish Audio{store.hasKey === true ? " (configurée)" : store.hasKey === false ? " (absente)" : ""}
    </span>
    <div class="api-key-row">
      <input
        type="password"
        bind:value={store.apiKey}
        placeholder="Coller la clé Fish Audio…"
        aria-label="Clé API Fish Audio"
        autocomplete="off"
        spellcheck={false}
      />
      <button type="button" class="primary" onclick={() => store.saveApiKey()}>Enregistrer</button>
      <button type="button" class="ghost" onclick={() => store.testApiKey()}>Tester</button>
    </div>
    {#if store.apiHint}<span class="field-hint">{store.apiHint}</span>{/if}
  </div>
</section>

<section class="panel" id="cfg-agent">
  <div class="card-head">
    <h2>Agent</h2>
    <p class="card-hint">Agent lancé avec votre dictée comme demande.</p>
  </div>
  <div class="check-row">
    <span>Agent</span>
    <Dropdown
      options={AGENT_OPTS}
      bind:value={store.agentName}
      label="Agent"
      title="Agent lancé par le mode Agent avec ta dictée comme demande"
      onselect={(v) => store.pushAgentName(v)}
    />
  </div>
  <p class="field-hint">Binaire repli : omp si l'agent choisi est absent.</p>
  {#if store.agentName === "custom"}
    <label class="check-row">
      <span>Commande agent</span>
      <input
        value={store.agentCommand}
        oninput={(e) => store.pushAgentCommand(e.currentTarget.value)}
        placeholder="ma-cli --prompt {prompt}"
        aria-label="Commande agent personnalisée, {prompt} = ta dictée"
      />
    </label>
  {/if}
  {#if store.agentStatus}
    <section class="agent-card" aria-label="Session agent">
      <div class="agent-head">
        <h2 class="agent-title">Session agent</h2>
        <span class="agent-badge" class:on={store.agentStatus.active} aria-live="polite">
          {store.agentStatus.active ? "Active" : "Aucune"}
        </span>
      </div>
      <dl class="agent-rows">
        <div class="agent-row">
          <dt>Agent</dt>
          <dd>{store.agentStatus.agent}</dd>
        </div>
        <div class="agent-row">
          <dt>Dossier</dt>
          <dd title={store.agentStatus.cwd}>{store.agentStatus.cwd}</dd>
        </div>
        <div class="agent-row">
          <dt>Dernier prompt</dt>
          <dd title={store.agentStatus.lastPrompt ?? undefined}>
            {store.agentStatus.lastPrompt ? truncate120(store.agentStatus.lastPrompt) : "—"}
          </dd>
        </div>
      </dl>
      <div class="row agent-actions">
        <button
          type="button"
          class="primary"
          onclick={() => store.openAgentTerminal()}
          disabled={store.agentLaunching}
          aria-busy={store.agentLaunching}
        >
          {#if store.agentLaunching}<span class="btn-spinner" aria-hidden="true"></span>{/if}
          {store.agentLaunching ? "Lancement…" : "Ouvrir le terminal"}
        </button>
        <button type="button" class="ghost" onclick={() => store.refreshAgentStatus()}>Actualiser</button>
      </div>
      {#if store.agentOpenMsg}<p class="field-hint" role="status">{store.agentOpenMsg}</p>{/if}
    </section>
  {/if}
</section>

<section class="panel" id="cfg-agents">
  <div class="card-head">
    <h2>Agents</h2>
    <p class="card-hint">Suivi des sessions via hooks (popup agent) — un statut par agent, installation via install-hook.sh.</p>
  </div>
  {#each store.agentHooks ?? [] as a (a.id)}
    <div class="check-row">
      <span>
        <i class="dot" style:background={a.installed ? "#22c55e" : "#f4505e"}></i>
        {a.name}
        <span class="field-hint"> — {a.installed ? "installé" : "non installé"} · {a.path}</span>
      </span>
      <button
        type="button"
        class="primary"
        disabled={store.agentBusy === a.id}
        aria-busy={store.agentBusy === a.id}
        onclick={() => store.installAgent(a.id)}
      >
        {store.agentBusy === a.id ? "Installation…" : a.installed ? "Réinstaller" : "Installer"}
      </button>
    </div>
  {/each}
  {#if store.agentHooks === null}<p class="field-hint">Statut hooks indisponible (backend en cours).</p>{/if}
  {#if store.agentMsg}<p class="field-hint" role="status">{store.agentMsg}</p>{/if}
</section>

<section class="panel" id="cfg-modes-inline">
  <div class="card-head">
    <h2>Voix par mode</h2>
    <p class="card-hint">
      Voix-texte par mode. Basculez la voix entre Local et Fish Audio
      — {store.hasKey === true ? "clé Fish configurée" : "sans clé Fish, Fish Audio retombe en Local"}.
    </p>
  </div>
  <ul class="mode-list">
    {#each store.backendModes ?? modesFallback as m (m.id)}
      <li class="mode-row">
        <label class="mode-pick">
          <input
            type="radio"
            name="whisp-mode"
            checked={store.activeMode.toLowerCase() === m.id}
            onchange={() => store.chooseMode(m.label)}
          />
          <span>{m.label}</span>
        </label>
        <Dropdown
          options={STT_PER_MODE.map((o) => ({ value: o.value, label: o.label }))}
          value={m.stt === "cloud_fish_audio" ? "cloud_fish_audio" : "local"}
          label={`Voix-texte pour ${m.label}`}
          title={store.hasKey === true ? "Moteur de transcription" : "Ajoutez votre clé Fish dans Configuration pour activer Fish Audio"}
          onselect={(v) => store.setModeStt(m, v as SttKind)}
        />
      </li>
    {/each}
  </ul>
</section>

<style>
  /* Le Dropdown custom occupe la moitié droite de la ligne (ex-select 55 %). */
  .check-row :global(.dropdown) {
    flex: 0 1 auto;
    margin-left: auto;
    max-width: 55%;
    min-width: 180px;
  }
  .mode-row :global(.dropdown) {
    flex: 0 1 auto;
    margin-left: auto;
    max-width: 220px;
    min-width: 150px;
  }
</style>
