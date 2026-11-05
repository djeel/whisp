import { fetchAppConfig, flushQueuedConfig, invoke, listen, queueConfig as sharedQueueConfig } from "./tauri";
import {
  applyAppearance,
  fetchCaelestiaScheme,
  isAccentHex,
  parseAccentSource,
  readAccentCustom,
  readAccentSource,
  startCaelestiaWatch,
  type AccentSource,
  type CaelestiaScheme,
} from "./accent";
import {
  getProviderConf,
  isChatProvider,
  loadProviderConfs,
  migrateStoredOllamaModel,
  saveProviderConfs,
  type ChatProviderId,
} from "./chatProviders";
import { readJSON, readPref, writeJSON, writePref } from "./prefs";

export type TabId = "home" | "stats" | "chat" | "modes" | "vocabulary" | "config";
export type ThemePref = "system" | "dark" | "light";
export type PopupPos = "top-center" | "center" | "bottom-center" | "cursor";
export type SttModel = "small" | "medium" | "base";
export type SttKind = "local" | "cloud_fish_audio";
export type PopupSize = "S" | "M" | "L";

export interface BackendMode {
  id: string;
  label: string;
  preset: string;
  prompt: string;
  stt: SttKind | string;
  llm: string;
  language: string;
  vocabulary?: string[];
}

export interface AgentHookEntry {
  id: string;
  name: string;
  installed: boolean;
  path: string;
}

export interface AgentStatus {
  active: boolean;
  agent: string;
  cwd: string;
  lastPrompt: string | null;
}

export interface Prefs {
  launchAtLogin: boolean;
  playSound: boolean;
  autoCopy: boolean;
  language: string;
}

export const FLOW_TABS: Array<{ id: TabId; label: string }> = [
  { id: "home", label: "Dictation" },
  { id: "stats", label: "Statistics" },
  { id: "vocabulary", label: "Dictionary" },
  { id: "modes", label: "Modes" },
  { id: "chat", label: "Chat" },
  { id: "config", label: "Settings" },
];

export const PILL_DEFS: Array<{ id: string; label: string; hint: string }> = [
  { id: "dictee", label: "Dictée", hint: "Transcription + collage au curseur" },
  { id: "agent", label: "Agent", hint: "Agent lancé avec la dictée comme demande" },
  { id: "agents", label: "Agents suivis", hint: "Sessions agents via hooks (popup agent)" },
  { id: "chat", label: "Chat", hint: "Modèle local ou cloud" },
];

export const MODES = ["Message", "Mail", "Voice", "Agent"];

export const POPUP_POS_OPTIONS: Array<{ id: PopupPos; label: string }> = [
  { id: "top-center", label: "Haut-centre (Superwhisper)" },
  { id: "center", label: "Centre" },
  { id: "bottom-center", label: "Bas-centre" },
  { id: "cursor", label: "Près du curseur" },
];

export const DEFAULT_PREFS: Prefs = {
  launchAtLogin: false,
  playSound: true,
  autoCopy: true,
  language: "fr",
};

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* stockage indisponible */
  }
}

export function isPopupPos(v: unknown): v is PopupPos {
  return v === "top-center" || v === "center" || v === "bottom-center" || v === "cursor";
}

function isPopupSize(v: unknown): v is PopupSize {
  return v === "S" || v === "M" || v === "L";
}

function isSttModel(v: unknown): v is SttModel {
  return v === "small" || v === "medium" || v === "base";
}

export function truncate120(s: string): string {
  return s.length > 120 ? `${s.slice(0, 120)}…` : s;
}

/** État Settings complet : 1 seul fetchAppConfig() au mount (single-flight),
 * queueConfig debounce 400 ms + flush explicite, migrations conservées. */
export class SettingsStore {
  tab = $state<TabId>(readStored("whisp:tab") === "stats" || readStored("whisp:tab") === "chat" || readStored("whisp:tab") === "modes" || readStored("whisp:tab") === "vocabulary" || readStored("whisp:tab") === "config" ? (readStored("whisp:tab") as TabId) : "home");
  refreshTick = $state(0);
  vocab = $state<string[]>(readJSON<string[]>("whisp:vocab", []));
  word = $state("");
  prefs = $state<Prefs>({ ...DEFAULT_PREFS, ...readJSON<Partial<Prefs>>("whisp:prefs", {}) });
  theme = $state<ThemePref>(readPref("whisp:theme") === "dark" || readPref("whisp:theme") === "light" ? (readPref("whisp:theme") as ThemePref) : "system");
  popupPos = $state<PopupPos>(isPopupPos(readPref("whisp:popup-pos")) ? (readPref("whisp:popup-pos") as PopupPos) : "top-center");
  sttModel = $state<SttModel>(isSttModel(readPref("whisp:stt-model")) ? (readPref("whisp:stt-model") as SttModel) : "small");
  apiKey = $state("");
  dictLang = $state<string>(readStored("whisp:dict-lang") ?? "fr");
  apiHint = $state<string | null>(null);
  hasKey = $state<boolean | null>(null);
  activeMode = $state<string>(readStored("whisp:mode") ?? "Auto");
  backendModes = $state<BackendMode[] | null>(null);
  vocabMsg = $state<string | null>(null);
  autoPaste = $state<boolean>(readStored("whisp:auto-paste") !== "0");
  realtime = $state<boolean>(readStored("whisp:realtime") !== "0");
  activePills = $state<string[]>(readJSON<string[]>("whisp:active-pills", ["dictee", "agent", "agents", "chat"]));
  pillDefs = PILL_DEFS;
  agentHooks = $state<AgentHookEntry[] | null>(null);
  agentMsg = $state<string | null>(null);
  agentBusy = $state<string | null>(null);
  chatProvider = $state<ChatProviderId>(isChatProvider(readPref("whisp:chat-provider")) ? (readPref("whisp:chat-provider") as ChatProviderId) : "ollama");
  providerConfs = $state<Record<string, { model: string; baseUrl: string }>>(loadProviderConfs());
  chatKey = $state("");
  chatKeyHint = $state<string | null>(null);
  chatKeyState = $state<boolean | null>(null);
  claudeToken = $state("");
  claudeHint = $state<string | null>(null);
  claudeStyle = $state<string>(readStored("whisp:claude-style") ?? "");
  claudeStyleHint = $state<string | null>(null);
  ollamaUrl = $state<string>(readStored("whisp:ollama-url") ?? "http://127.0.0.1:11434");
  ollamaModel = $state<string>(migrateStoredOllamaModel(readStored("whisp:ollama-model")));
  ollamaMsg = $state<string | null>(null);
  ollamaBusy = $state(false);
  inputDevices = $state<string[]>([]);
  inputDevice = $state<string>(readStored("whisp:input-device") ?? "");
  agentName = $state<string>(readStored("whisp:agent") ?? readStored("whisp:sta-agent") ?? "omp");
  agentCommand = $state<string>(readStored("whisp:agent-command") ?? readStored("whisp:sta-command") ?? "");
  agentStatus = $state<AgentStatus | null>(null);
  agentOpenMsg = $state<string | null>(null);
  agentLaunching = $state(false);
  voice = $state<string>(readStored("whisp:fish-voice") ?? "");
  agentVoice = $state<boolean>((readStored("whisp:agent-voice") ?? readStored("whisp:sta-voice")) === "1");
  voiceMsg = $state<string | null>(null);
  voiceTesting = $state(false);
  size = $state<PopupSize>(isPopupSize(readStored("whisp:popup-size")) ? (readStored("whisp:popup-size") as PopupSize) : "M");
  opacity = $state<number>(typeof Number(readStored("whisp:card-opacity")) === "number" && Number(readStored("whisp:card-opacity")) >= 0.4 && Number(readStored("whisp:card-opacity")) <= 1 ? Number(readStored("whisp:card-opacity")) : 1);
  accentSource = $state<AccentSource>(readAccentSource());
  accentCustom = $state<string>(readAccentCustom());
  caelestia = $state<CaelestiaScheme | null>(null);
  sizeMsg = $state<string | null>(null);

  chatModel = $derived(getProviderConf(this.providerConfs, this.chatProvider).model);
  chatBaseUrl = $derived(getProviderConf(this.providerConfs, this.chatProvider).baseUrl);
  activeModeEntry = $derived<BackendMode | null>(
    this.backendModes && this.backendModes.length > 0
      ? (this.backendModes.find((m) => m.label.toLowerCase() === this.activeMode.trim().toLowerCase()) ??
        this.backendModes.find((m) => m.id === this.activeMode.trim().toLowerCase()) ??
        this.backendModes[0])
      : null,
  );

  private unlistenCaelestia: (() => void) | null = null;
  private live = false;

  // Debounce partagé (tauri.ts, single timer 400 ms) : pas de 2e set_config.
  queueConfig(patch: Record<string, unknown>): void {
    sharedQueueConfig(patch);
  }

  flushConfig(): Promise<void> {
    return flushQueuedConfig();
  }

  /** Mount : UN SEUL fetchAppConfig (single-flight partagé), puis sync indépendants. */
  async init(): Promise<void> {
    this.live = true;
    const cfg = (await fetchAppConfig()) as Record<string, unknown>;
    if (!this.live) return;
    if (isPopupPos(cfg?.popupPos)) this.popupPos = cfg.popupPos as PopupPos;
    if (isSttModel(cfg?.sttModel)) {
      this.sttModel = cfg.sttModel as SttModel;
      writePref("whisp:stt-model", this.sttModel);
    }
    if (cfg?.theme === "system" || cfg?.theme === "dark" || cfg?.theme === "light") this.theme = cfg.theme as ThemePref;
    if (cfg?.language === "fr" || cfg?.language === "en" || cfg?.language === "auto") {
      this.dictLang = cfg.language as string;
      writeStored("whisp:dict-lang", this.dictLang);
    }
    if (typeof cfg?.inputDevice === "string") {
      this.inputDevice = cfg.inputDevice as string;
      writeStored("whisp:input-device", this.inputDevice);
    }
    if (typeof cfg?.autoPaste === "boolean") {
      this.autoPaste = cfg.autoPaste as boolean;
      writeStored("whisp:auto-paste", this.autoPaste ? "1" : "0");
    }
    if (typeof cfg?.realtimePreview === "boolean") {
      this.realtime = cfg.realtimePreview as boolean;
      writeStored("whisp:realtime", this.realtime ? "1" : "0");
    }
    if (typeof cfg?.agent === "string" && cfg.agent) {
      this.agentName = cfg.agent as string;
      writeStored("whisp:agent", this.agentName);
    }
    if (typeof cfg?.agentCommand === "string") {
      this.agentCommand = cfg.agentCommand as string;
      writeStored("whisp:agent-command", this.agentCommand);
    }
    if (Array.isArray(cfg?.activePills)) {
      const kept = (cfg.activePills as unknown[]).filter((p): p is string => typeof p === "string").map((p) => (p === "sta" ? "agent" : p));
      if (kept.length > 0) {
        this.activePills = kept;
        writeJSON("whisp:active-pills", kept);
      }
    }
    if (typeof cfg?.ollamaUrl === "string" && (cfg.ollamaUrl as string).trim()) {
      this.ollamaUrl = cfg.ollamaUrl as string;
      writeStored("whisp:ollama-url", this.ollamaUrl);
    }
    if (typeof cfg?.ollamaModel === "string" && (cfg.ollamaModel as string).trim()) {
      this.ollamaModel = migrateStoredOllamaModel(cfg.ollamaModel as string);
      writeStored("whisp:ollama-model", this.ollamaModel);
    }
    if (typeof cfg?.chatProvider === "string" && isChatProvider(cfg.chatProvider)) {
      this.chatProvider = cfg.chatProvider;
      writePref("whisp:chat-provider", this.chatProvider);
    }
    if (typeof cfg?.claudeStyle === "string" && (cfg.claudeStyle as string).trim()) {
      this.claudeStyle = cfg.claudeStyle as string;
      writeStored("whisp:claude-style", this.claudeStyle);
    }
    if (typeof cfg?.chatModel === "string" || typeof cfg?.chatBaseUrl === "string") {
      const prov = typeof cfg?.chatProvider === "string" && isChatProvider(cfg.chatProvider) ? (cfg.chatProvider as string) : "ollama";
      const cur = getProviderConf(this.providerConfs, prov);
      let model = typeof cfg?.chatModel === "string" ? (cfg.chatModel as string) : cur.model;
      if (prov === "ollama") model = migrateStoredOllamaModel(model || null);
      const confs = { ...this.providerConfs, [prov]: { model, baseUrl: typeof cfg?.chatBaseUrl === "string" ? (cfg.chatBaseUrl as string) : cur.baseUrl } };
      this.providerConfs = confs;
      saveProviderConfs(confs);
    }
    // Migration ancien défaut local (qwen2.5:3b → Nanbeige) : une fois au mount.
    const curOllama = this.providerConfs["ollama"];
    if (curOllama && (curOllama.model === "qwen2.5:3b" || curOllama.model === "ndavat/Nanbeige4.2-3B")) {
      const confs = { ...this.providerConfs, ollama: { model: "whisp-nanbeige", baseUrl: curOllama.baseUrl ?? "" } };
      this.providerConfs = confs;
      saveProviderConfs(confs);
    }
    if (typeof cfg?.fishVoice === "string" && cfg.fishVoice) {
      this.voice = cfg.fishVoice as string;
      writeStored("whisp:fish-voice", this.voice);
    }
    if (typeof cfg?.agentVoice === "boolean") {
      this.agentVoice = cfg.agentVoice as boolean;
      writeStored("whisp:agent-voice", this.agentVoice ? "1" : "0");
    }
    if (isPopupSize(cfg?.popupSize)) {
      this.size = cfg.popupSize as PopupSize;
      writeStored("whisp:popup-size", this.size);
    }
    if (typeof cfg?.cardOpacity === "number" && (cfg.cardOpacity as number) >= 0.4 && (cfg.cardOpacity as number) <= 1) {
      this.opacity = cfg.cardOpacity as number;
      writeStored("whisp:card-opacity", String(this.opacity));
    }
    const src = parseAccentSource(cfg?.accentSource);
    const srcNext = src ?? readAccentSource();
    const customNext = isAccentHex(cfg?.accentCustom) ? (cfg.accentCustom as string).toLowerCase() : readAccentCustom();
    this.accentSource = srcNext;
    this.accentCustom = customNext;
    try {
      document.documentElement.style.setProperty("--card-alpha", String(Number(readStored("whisp:card-opacity") ?? "1") || 1));
    } catch {
      /* pas de DOM */
    }
    // Accent : backend source de vérité, puis suivi live du fond d'écran.
    const scheme = await fetchCaelestiaScheme().catch(() => null);
    if (!this.live) return;
    this.caelestia = scheme;
    startCaelestiaWatch();
    try {
      const un = await listen<CaelestiaScheme>("caelestia://scheme", (e) => {
        if (!this.live) return;
        const next = e.payload.available ? e.payload : null;
        this.caelestia = next;
        // Lecture live de this.theme : pas de closure stale (ex-themeRef).
        applyAppearance(this.theme === "system" ? null : this.theme, readAccentSource(), readAccentCustom(), next);
      });
      if (!this.live) {
        un();
        return;
      }
      this.unlistenCaelestia = un;
    } catch {
      /* navigateur : pas d'événements backend */
    }
    // Syncs indépendants (pas du get_config) : clés, hooks, modes, devices.
    invoke<boolean>("has_api_key", { provider: "fish_audio" })
      .then((v) => {
        if (this.live) this.hasKey = v === true;
      })
      .catch(() => {
        if (this.live) this.hasKey = null;
      });
    invoke<AgentHookEntry[]>("agent_hook_status")
      .then((rows) => {
        if (this.live && Array.isArray(rows)) this.agentHooks = rows;
      })
      .catch(() => {
        if (this.live) this.agentHooks = null;
      });
    this.refreshAgentStatus();
    this.refreshChatKeyState();
    invoke<BackendMode[]>("list_modes")
      .then((ms) => {
        if (this.live && Array.isArray(ms)) this.backendModes = ms;
      })
      .catch(() => {
        if (this.live) this.backendModes = null;
      });
    invoke<string[]>("list_input_devices")
      .then((ds) => {
        if (this.live && Array.isArray(ds)) this.inputDevices = ds;
      })
      .catch(() => {
        if (this.live) this.inputDevices = [];
      });
  }

  destroy(): void {
    this.live = false;
    void this.flushConfig();
    if (this.unlistenCaelestia) this.unlistenCaelestia();
    this.unlistenCaelestia = null;
  }

  goTab(next: TabId): void {
    this.tab = next;
    if (next === "home") this.refreshTick += 1;
    writeStored("whisp:tab", next);
  }

  refreshChatKeyState(): void {
    if (this.chatProvider === "ollama") {
      this.chatKeyState = null;
      return;
    }
    invoke<boolean>("has_api_key", { provider: this.chatProvider })
      .then((v) => {
        if (!this.live) return;
        this.chatKeyState = v === true;
        if (this.chatProvider === "claude" && v === true) this.claudeHint = "Compte Claude connecté.";
      })
      .catch(() => {
        if (this.live) this.chatKeyState = null;
      });
  }

  refreshAgentStatus(): void {
    invoke<AgentStatus>("agent_status")
      .then((s) => {
        if (this.live) this.agentStatus = s;
      })
      .catch(() => {
        /* backend en cours : carte masquée */
      });
  }

  addWord(): void {
    const w = this.word.trim();
    if (!w || this.vocab.includes(w)) return;
    this.vocab = [...this.vocab, w];
    writeJSON("whisp:vocab", this.vocab);
    this.word = "";
  }

  removeWord(index: number): void {
    this.vocab = this.vocab.filter((_, i) => i !== index);
    writeJSON("whisp:vocab", this.vocab);
  }

  applyVocabToActiveMode(): void {
    const target = this.activeModeEntry;
    if (!target) {
      this.vocabMsg = "Modes indisponibles (backend en cours).";
      return;
    }
    const updated: BackendMode = { ...target, vocabulary: [...this.vocab] };
    this.backendModes = (this.backendModes ?? []).map((m) => (m.id === target.id ? updated : m));
    invoke<BackendMode>("upsert_mode", { mode: updated })
      .then((saved) => {
        this.backendModes = (this.backendModes ?? []).map((m) => (m.id === saved.id ? saved : m));
        this.vocabMsg = `Vocabulaire appliqué à ${saved.label}.`;
      })
      .catch(() => {
        this.vocabMsg = "Backend en cours : vocabulaire gardé en local.";
      });
  }

  patchPrefs(patch: Partial<Prefs>): void {
    this.prefs = { ...this.prefs, ...patch };
    writeJSON("whisp:prefs", this.prefs);
  }

  chooseMode(m: string): void {
    this.activeMode = m;
    writeStored("whisp:mode", m);
    const id = (this.backendModes ?? []).find((x) => x.label === m)?.id ?? m.toLowerCase();
    invoke<BackendMode>("set_mode", { id }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  }

  setModeStt(mode: BackendMode, stt: SttKind): void {
    const updated: BackendMode = { ...mode, stt };
    this.backendModes = (this.backendModes ?? []).map((m) => (m.id === mode.id ? updated : m));
    invoke<BackendMode>("upsert_mode", { mode: updated })
      .then((saved) => {
        this.backendModes = (this.backendModes ?? []).map((m) => (m.id === saved.id ? saved : m));
      })
      .catch(() => {
        /* backend en cours : rollback visuel au prochain chargement */
      });
  }

  pushPopupPos(next: PopupPos): void {
    this.popupPos = next;
    writePref("whisp:popup-pos", next);
    this.queueConfig({ popupPos: next });
  }

  pushTheme(next: ThemePref): void {
    this.theme = next;
    this.queueConfig({ theme: next });
  }

  pushSttModel(next: SttModel): void {
    this.sttModel = next;
    writePref("whisp:stt-model", next);
    this.queueConfig({ sttModel: next });
  }

  pushDictLang(next: string): void {
    this.dictLang = next;
    writeStored("whisp:dict-lang", next);
    this.queueConfig({ language: next });
  }

  pushAutoPaste(next: boolean): void {
    this.autoPaste = next;
    writeStored("whisp:auto-paste", next ? "1" : "0");
    this.queueConfig({ autoPaste: next });
  }

  pushRealtime(next: boolean): void {
    this.realtime = next;
    writeStored("whisp:realtime", next ? "1" : "0");
    this.queueConfig({ realtimePreview: next });
  }

  pushInputDevice(next: string): void {
    this.inputDevice = next;
    writeStored("whisp:input-device", next);
    this.queueConfig({ inputDevice: next || null });
  }

  pushAgentName(next: string): void {
    this.agentName = next;
    writeStored("whisp:agent", next);
    this.queueConfig({ agent: next });
  }

  pushAgentCommand(next: string): void {
    this.agentCommand = next;
    writeStored("whisp:agent-command", next);
    this.queueConfig({ agentCommand: next || null });
  }

  togglePill(id: string): void {
    const next = this.activePills.includes(id) ? this.activePills.filter((p) => p !== id) : [...this.activePills, id];
    this.activePills = next;
    writeJSON("whisp:active-pills", next);
    this.queueConfig({ activePills: next });
  }

  installAgent(agent: string): void {
    this.agentBusy = agent;
    this.agentMsg = null;
    invoke<string>("install_agent_hooks", { agent })
      .then((out) => {
        this.agentMsg = out || `Hooks installés (${agent}).`;
        return invoke<AgentHookEntry[]>("agent_hook_status");
      })
      .then((rows) => {
        if (Array.isArray(rows)) this.agentHooks = rows;
      })
      .catch((e: unknown) => {
        this.agentMsg = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        this.agentBusy = null;
      });
  }

  pushChatProvider(next: string): void {
    if (!isChatProvider(next)) return;
    this.chatProvider = next;
    writePref("whisp:chat-provider", next);
    this.queueConfig({ chatProvider: next });
    this.refreshChatKeyState();
  }

  pushChatModel(next: string): void {
    const confs = { ...this.providerConfs, [this.chatProvider]: { model: next, baseUrl: getProviderConf(this.providerConfs, this.chatProvider).baseUrl } };
    this.providerConfs = confs;
    saveProviderConfs(confs);
    this.queueConfig({ chatModel: next, chatProvider: this.chatProvider });
  }

  pushChatBaseUrl(next: string): void {
    const confs = { ...this.providerConfs, [this.chatProvider]: { model: getProviderConf(this.providerConfs, this.chatProvider).model, baseUrl: next } };
    this.providerConfs = confs;
    saveProviderConfs(confs);
    this.queueConfig({ chatBaseUrl: next, chatProvider: this.chatProvider });
  }

  saveChatKey(): void {
    const key = this.chatKey.trim();
    if (this.chatProvider === "claude") {
      this.saveClaudeToken();
      return;
    }
    if (!key || this.chatProvider === "ollama") {
      this.chatKeyHint = this.chatProvider === "ollama" ? "Ollama local : aucune clé requise." : "Collez d'abord votre clé.";
      return;
    }
    invoke("set_api_key", { provider: this.chatProvider, key })
      .then(() => {
        this.chatKeyState = true;
        this.chatKey = "";
        this.chatKeyHint = "Clé enregistrée.";
      })
      .catch(() => {
        this.chatKeyHint = "Backend en cours : commande set_api_key indisponible.";
      });
  }

  saveClaudeToken(): void {
    const token = (this.chatProvider === "claude" ? this.chatKey : this.claudeToken).trim();
    if (!token) {
      if (this.chatProvider === "claude") this.chatKeyHint = "Collez le token `claude setup-token`.";
      else this.claudeHint = "Collez le token `claude setup-token`.";
      return;
    }
    invoke("set_api_key", { provider: "claude_oauth", key: token })
      .then(() => {
        this.chatKeyState = true;
        this.chatKey = "";
        this.claudeToken = "";
        this.chatKeyHint = "Compte Claude connecté.";
        this.claudeHint = "Compte Claude connecté.";
      })
      .catch((e: unknown) => {
        const msg = e instanceof Error ? e.message : String(e);
        if (this.chatProvider === "claude") this.chatKeyHint = msg;
        else this.claudeHint = msg;
      });
  }

  pushClaudeStyle(next: string): void {
    this.claudeStyle = next;
    writeStored("whisp:claude-style", next);
    this.queueConfig({ claudeStyle: next });
    this.claudeStyleHint = next.trim() ? "Style enregistré — appliqué au chat + raffinage." : null;
  }

  pushOllamaUrl(next: string): void {
    this.ollamaUrl = next;
    writeStored("whisp:ollama-url", next);
    try {
      const u = new URL(next.trim());
      if (u.protocol === "http:" || u.protocol === "https:") this.queueConfig({ ollamaUrl: next });
    } catch {
      /* URL incomplète pendant la frappe : on attend */
    }
  }

  pushOllamaModel(next: string): void {
    this.ollamaModel = next;
    writeStored("whisp:ollama-model", next);
    this.queueConfig({ ollamaModel: next });
  }

  testOllama(): void {
    this.ollamaBusy = true;
    this.ollamaMsg = null;
    invoke<string>("test_ollama", { url: this.ollamaUrl || null })
      .then((out) => {
        this.ollamaMsg = out;
      })
      .catch((e: unknown) => {
        this.ollamaMsg = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        this.ollamaBusy = false;
      });
  }

  openAgentTerminal(): void {
    const agent = this.agentName || "omp";
    this.agentOpenMsg = null;
    this.agentLaunching = true;
    invoke("agent_open_terminal")
      .then(() => {
        this.agentOpenMsg = `Agent lancé : ${agent}`;
        this.refreshAgentStatus();
      })
      .catch(() => {
        this.agentOpenMsg = "Agent introuvable, vérifie Configuration.";
      })
      .finally(() => {
        this.agentLaunching = false;
      });
  }

  saveApiKey(): void {
    const key = this.apiKey.trim();
    if (!key) {
      this.apiHint = "Collez d’abord votre clé.";
      return;
    }
    invoke("set_api_key", { provider: "fish_audio", key })
      .then(() => {
        this.hasKey = true;
        this.apiKey = "";
        this.apiHint = "Clé enregistrée.";
      })
      .catch(() => {
        this.apiHint = "Backend en cours : commande set_api_key indisponible.";
      });
  }

  testApiKey(): void {
    invoke<boolean>("has_api_key", { provider: "fish_audio" })
      .then((v) => {
        this.hasKey = v === true;
        this.apiHint = v === true ? "Clé détectée (stockée côté backend)." : "Aucune clé stockée.";
      })
      .catch(() => {
        this.apiHint = "Backend en cours : commande has_api_key indisponible.";
      });
  }

  pushVoice(next: string): void {
    this.voice = next;
    writeStored("whisp:fish-voice", next);
    this.queueConfig({ fishVoice: next || null });
  }

  pushAgentVoice(next: boolean): void {
    this.agentVoice = next;
    writeStored("whisp:agent-voice", next ? "1" : "0");
    invoke("set_config", { agentVoice: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  }

  testVoice(): void {
    if (this.hasKey === false) {
      this.voiceMsg = "Ajoutez d'abord votre clé Fish dans Configuration.";
      return;
    }
    if (!this.voice.trim()) {
      this.voiceMsg = "Collez d'abord un reference_id Fish (console fish.audio → TTS → voix).";
      return;
    }
    this.voiceTesting = true;
    this.voiceMsg = null;
    invoke("speak_text", { text: "Bonjour", voice: this.voice.trim() || null })
      .then(() => {
        this.voiceMsg = "Lecture en cours…";
      })
      .catch((e) => {
        this.voiceMsg = `Échec du test : ${String(e)}`;
      })
      .finally(() => {
        this.voiceTesting = false;
      });
  }

  pushSize(next: PopupSize): void {
    this.size = next;
    writeStored("whisp:popup-size", next);
    this.sizeMsg = null;
    this.queueConfig({ popupSize: next });
    invoke<string>("set_popup_size", { size: next })
      .then((m) => {
        this.sizeMsg = m;
      })
      .catch(() => {
        this.sizeMsg = "Taille mémorisée (popup injoignable).";
      });
  }

  pushOpacity(next: number): void {
    this.opacity = next;
    writeStored("whisp:card-opacity", String(next));
    try {
      document.documentElement.style.setProperty("--card-alpha", String(next));
    } catch {
      /* pas de DOM */
    }
    this.queueConfig({ cardOpacity: next });
  }

  pushAccentSource(next: AccentSource): void {
    this.accentSource = next;
    writeStored("whisp:accent-source", next);
    this.queueConfig({ accentSource: next });
  }

  pushAccentCustom(raw: string): void {
    const next = raw.toLowerCase();
    this.accentCustom = next;
    if (!isAccentHex(next)) return;
    writeStored("whisp:accent-custom", next);
    this.queueConfig({ accentCustom: next });
  }
}
