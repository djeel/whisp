import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Chat from "./components/Chat";
import Mochi, { type MochiMood } from "./components/Mochi";
import Recorder from "./components/Recorder";
import Stats from "./components/Stats";
import Island from "./island/Island";
import Wardrobe from "./components/Wardrobe";
import { readOutfit, writeOutfit, type OutfitId } from "./mochi/types";
import "./App.css";

type TabId = "home" | "stats" | "chat" | "modes" | "vocabulary" | "config";
type ThemePref = "system" | "dark" | "light";
type PopupPos = "top-center" | "center" | "bottom-center" | "cursor";
type SttModel = "small" | "medium" | "base";

interface BackendConfig {
  popupPos?: unknown;
  sttModel?: unknown;
  theme?: unknown;
  language?: unknown;
  inputDevice?: unknown;
  staAgent?: unknown;
  staCommand?: unknown;
}

type SttKind = "local" | "cloud_fish_audio";

interface BackendMode {
  id: string;
  label: string;
  preset: string;
  prompt: string;
  stt: SttKind | string;
  llm: string;
  language: string;
  vocabulary?: string[];
}
const TABS: Array<{ id: TabId; label: string }> = [
  { id: "home", label: "Home" },
  { id: "stats", label: "Stats" },
  { id: "chat", label: "Chat" },
  { id: "modes", label: "Modes" },
  { id: "vocabulary", label: "Vocabulary" },
  { id: "config", label: "Configuration" },
];

/** Interrupteur custom (remplace la checkbox native) : 40x22px, 150ms, AA. */
function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}): React.JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`switch${checked ? " on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-knob" aria-hidden="true" />
    </button>
  );
}

const MODES = ["Message", "Super", "Mail", "Note", "Voice", "Meeting"];

const POPUP_POS_OPTIONS: Array<{ id: PopupPos; label: string }> = [
  { id: "top-center", label: "Haut-centre (Superwhisper)" },
  { id: "center", label: "Centre" },
  { id: "bottom-center", label: "Bas-centre" },
  { id: "cursor", label: "Près du curseur" },
];

interface HistoryEntry {
  id: string;
  timestamp: number;
  mode: string;
  transcript_raw: string;
  text_final: string;
  audio_path?: string | null;
}

interface Prefs {
  launchAtLogin: boolean;
  playSound: boolean;
  autoCopy: boolean;
  language: string;
}

const DEFAULT_PREFS: Prefs = {
  launchAtLogin: false,
  playSound: true,
  autoCopy: true,
  language: "fr",
};

interface StaStatus {
  active: boolean;
  agent: string;
  cwd: string;
  lastPrompt: string | null;
}

function truncate120(s: string): string {
  return s.length > 120 ? `${s.slice(0, 120)}…` : s;
}

function readJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function readStoredJSON(key: string): string | null {
  const raw = readStored(key);
  if (raw == null) return null;
  try {
    const v = JSON.parse(raw) as unknown;
    return typeof v === "string" ? v : raw;
  } catch {
    return raw;
  }
}

function writeJSON(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable */
  }
}

async function safeInvoke(cmd: string): Promise<void> {
  try {
    await invoke(cmd);
  } catch {
    /* browser dev without backend */
  }
}

function formatHistoryDate(timestamp: number): string {
  try {
    return new Date(timestamp * 1000).toLocaleString();
  } catch {
    return "";
  }
}

function isPopupPos(v: unknown): v is PopupPos {
  return v === "top-center" || v === "center" || v === "bottom-center" || v === "cursor";
}

export default function App() {
  const win = new URLSearchParams(window.location.search).get("window");

  useEffect(() => {
    if (win === "recorder") {
      document.documentElement.classList.add("is-recorder");
      document.body.classList.add("is-recorder");
    } else if (win === "island") {
      document.documentElement.classList.add("is-island");
      document.body.classList.add("is-island");
    } else {
      document.documentElement.dataset.theme = readStoredJSON("whisp:theme") ?? "system";
    }
  }, [win]);

  if (win === "recorder") return <Recorder />;
  if (win === "island") return <Island />;
  return <Settings />;
}

function Settings() {
  const [tab, setTab] = useState<TabId>("home");
  const [vocab, setVocab] = useState<string[]>(() => readJSON<string[]>("whisp:vocab", []));
  const [word, setWord] = useState("");
  const [prefs, setPrefs] = useState<Prefs>(() => ({
    ...DEFAULT_PREFS,
    ...readJSON<Partial<Prefs>>("whisp:prefs", {}),
  }));
  const [theme, setTheme] = useState<ThemePref>(() => {
    const raw = readStoredJSON("whisp:theme");
    return raw === "dark" || raw === "light" ? raw : "system";
  });
  const [popupPos, setPopupPos] = useState<PopupPos>(() => {
    const raw = readStoredJSON("whisp:popup-pos");
    return isPopupPos(raw) ? raw : "top-center";
  });
  const [sttModel, setSttModel] = useState<SttModel>(() => {
    const raw = readStoredJSON("whisp:stt-model");
    return raw === "medium" || raw === "base" ? raw : "small";
  });
  const [apiKey, setApiKey] = useState("");
  const [dictLang, setDictLang] = useState<string>(() => readStored("whisp:dict-lang") ?? "fr");
  const [apiHint, setApiHint] = useState<string | null>(null);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [activeMode, setActiveMode] = useState<string>(() => readStored("whisp:mode") ?? "Auto");
  const [backendModes, setBackendModes] = useState<BackendMode[] | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [vocabMsg, setVocabMsg] = useState<string | null>(null);
  const [outfit, setOutfit] = useState<OutfitId>(() => readOutfit());
  const [chatMood] = useState<MochiMood>("idle");

  // Theme: apply + persist; recorder window reads the same key on mount.
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    writeJSON("whisp:theme", theme);
  }, [theme]);

  // Backend config: sync once on mount (backend is source of truth when reachable).
  useEffect(() => {
    void invoke<BackendConfig>("get_config")
      .then((cfg) => {
        if (cfg && isPopupPos(cfg.popupPos)) setPopupPos(cfg.popupPos);
        if (cfg && (cfg.sttModel === "small" || cfg.sttModel === "medium" || cfg.sttModel === "base")) {
          setSttModel(cfg.sttModel);
        }
        if (cfg && (cfg.theme === "system" || cfg.theme === "dark" || cfg.theme === "light")) {
          setTheme(cfg.theme);
        }
        if (cfg && (cfg.language === "fr" || cfg.language === "en" || cfg.language === "auto")) {
          setDictLang(cfg.language);
        }
        if (typeof cfg?.inputDevice === "string") {
          setInputDevice(cfg.inputDevice);
        }
        if (typeof cfg?.staAgent === "string" && cfg.staAgent) {
          setStaAgent(cfg.staAgent);
        }
        if (typeof cfg?.staCommand === "string") {
          setStaCommand(cfg.staCommand);
        }
      })
      .catch(() => {
        /* backend en cours : localStorage seul */
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    invoke<boolean>("has_api_key", { provider: "fish_audio" })
      .then((v) => setHasKey(v === true))
      .catch(() => setHasKey(null));
    refreshStaStatus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    invoke<BackendMode[]>("list_modes")
      .then((ms) => {
        if (Array.isArray(ms)) setBackendModes(ms);
      })
      .catch(() => setBackendModes(null));
  }, []);
  useEffect(() => {
    invoke<string[]>("list_input_devices")
      .then((ds) => {
        if (Array.isArray(ds)) setInputDevices(ds);
      })
      .catch(() => setInputDevices([]));
  }, []);
  const refreshHistory = useCallback((): void => {
    invoke<HistoryEntry[]>("list_history", { limit: 50 })
      .then((rows) => {
        if (Array.isArray(rows)) setHistory(rows);
      })
      .catch(() => {
        /* backend en cours : historique vide */
      });
  }, []);
  useEffect(() => {
    refreshHistory();
    // Migration : ignorer l'ancienne clé locale jamais écrite par les dictées.
    try {
      localStorage.removeItem("whisp:history");
    } catch {
      /* stockage indisponible */
    }
  }, [refreshHistory]);
  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | null = null;
    void listen<unknown>("whisp://state", (e) => {
      if (cancelled) return;
      const p = e.payload as { status?: unknown } | null;
      if (p && typeof p === "object" && p.status === "done") refreshHistory();
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          off = unlisten;
        }
      })
      .catch(() => {
        /* navigateur : pas d'événements backend */
      });
    return () => {
      cancelled = true;
      if (off) off();
    };
  }, [refreshHistory]);

  const addWord = (): void => {
    const w = word.trim();
    if (!w || vocab.includes(w)) return;
    const next = [...vocab, w];
    setVocab(next);
    writeJSON("whisp:vocab", next);
    setWord("");
  };

  const removeWord = (index: number): void => {
    const next = vocab.filter((_, i) => i !== index);
    setVocab(next);
    writeJSON("whisp:vocab", next);
  };
  const activeModeEntry: BackendMode | null =
    backendModes && backendModes.length > 0
      ? (backendModes.find((m) => m.label.toLowerCase() === activeMode.trim().toLowerCase()) ??
        backendModes.find((m) => m.id === activeMode.trim().toLowerCase()) ??
        backendModes[0])
      : null;
  const applyVocabToActiveMode = useCallback((): void => {
    const target =
      backendModes && backendModes.length > 0
        ? (backendModes.find((m) => m.label.toLowerCase() === activeMode.trim().toLowerCase()) ??
          backendModes.find((m) => m.id === activeMode.trim().toLowerCase()) ??
          backendModes[0])
        : null;
    if (!target) {
      setVocabMsg("Modes indisponibles (backend en cours).");
      return;
    }
    const updated: BackendMode = { ...target, vocabulary: [...vocab] };
    setBackendModes((prev) => (prev ?? []).map((m) => (m.id === target.id ? updated : m)));
    invoke<BackendMode>("upsert_mode", { mode: updated })
      .then((saved) => {
        setBackendModes((prev) => (prev ?? []).map((m) => (m.id === saved.id ? saved : m)));
        setVocabMsg(`Vocabulaire appliqué à ${saved.label}.`);
      })
      .catch(() => {
        setVocabMsg("Backend en cours : vocabulaire gardé en local.");
      });
  }, [activeMode, backendModes, vocab]);

  const patchPrefs = (patch: Partial<Prefs>): void => {
    const next = { ...prefs, ...patch };
    setPrefs(next);
    writeJSON("whisp:prefs", next);
  };

  const chooseMode = (m: string): void => {
    setActiveMode(m);
    try {
      localStorage.setItem("whisp:mode", m);
    } catch {
      /* storage unavailable */
    }
  };
  const setModeStt = (mode: BackendMode, stt: SttKind): void => {
    const updated: BackendMode = { ...mode, stt };
    setBackendModes((prev) => (prev ?? []).map((m) => (m.id === mode.id ? updated : m)));
    invoke<BackendMode>("upsert_mode", { mode: updated })
      .then((saved) => {
        setBackendModes((prev) => (prev ?? []).map((m) => (m.id === saved.id ? saved : m)));
      })
      .catch(() => {
        /* backend en cours : rollback visuel au prochain chargement */
      });
  };

  const deleteEntry = useCallback((id: string): void => {
    // Optimiste : retire tout de suite (jamais de freeze UI).
    // Rollback si le backend échoue (ligne restaurée).
    setHistory((prev) => {
      const gone = prev.find((h) => h.id === id);
      if (!gone) return prev;
      void invoke("delete_history", { id }).catch(() => {
        setHistory((cur) => (cur.some((h) => h.id === id) ? cur : [...cur, gone]));
      });
      return prev.filter((h) => h.id !== id);
    });
  }, []);

  const pushPopupPos = (next: PopupPos): void => {
    setPopupPos(next);
    writeJSON("whisp:popup-pos", next);
    invoke("set_config", { popupPos: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const pushTheme = (next: ThemePref): void => {
    setTheme(next);
    invoke("set_config", { theme: next }).catch(() => {
      /* backend en cours : localStorage seul (effet useEffect persiste déjà) */
    });
  };

  const pushSttModel = (next: SttModel): void => {
    setSttModel(next);
    writeJSON("whisp:stt-model", next);
    invoke("set_config", { sttModel: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };
  const pushDictLang = (next: string): void => {
    setDictLang(next);
    try {
      localStorage.setItem("whisp:dict-lang", next);
    } catch {
      /* stockage indisponible */
    }
    invoke("set_config", { language: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };
  const [inputDevices, setInputDevices] = useState<string[]>([]);
  const [inputDevice, setInputDevice] = useState<string>(() => readStored("whisp:input-device") ?? "");

  const pushInputDevice = (next: string): void => {
    setInputDevice(next);
    try {
      localStorage.setItem("whisp:input-device", next);
    } catch {
      /* stockage indisponible */
    }
    invoke("set_config", { inputDevice: next || null }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };
  const [staAgent, setStaAgent] = useState<string>(() => readStored("whisp:sta-agent") ?? "omp");
  const [staCommand, setStaCommand] = useState<string>(() =>
    readStored("whisp:sta-command") ?? "",
  );

  const pushStaAgent = (next: string): void => {
    setStaAgent(next);
    try {
      localStorage.setItem("whisp:sta-agent", next);
    } catch {
      /* stockage indisponible */
    }
    invoke("set_config", { staAgent: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const pushStaCommand = (next: string): void => {
    setStaCommand(next);
    try {
      localStorage.setItem("whisp:sta-command", next);
    } catch {
      /* stockage indisponible */
    }
    invoke("set_config", { staCommand: next || null }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };
  const [staStatus, setStaStatus] = useState<StaStatus | null>(null);
  const [staOpenMsg, setStaOpenMsg] = useState<string | null>(null);

  const refreshStaStatus = (): void => {
    invoke<StaStatus>("sta_status")
      .then((s) => setStaStatus(s))
      .catch(() => {
        /* backend STA en cours : carte masquée */
      });
  };

  const [staLaunching, setStaLaunching] = useState(false);

  const openStaTerminal = (): void => {
    const agent = staAgent || "omp";
    setStaOpenMsg(null);
    setStaLaunching(true);
    invoke("sta_open_terminal")
      .then(() => {
        setStaOpenMsg(`Agent lancé : ${agent}`);
        refreshStaStatus();
      })
      .catch(() => setStaOpenMsg("Agent introuvable, vérifie Configuration."))
      .finally(() => setStaLaunching(false));
  };


  const saveApiKey = (): void => {
    const key = apiKey.trim();
    if (!key) {
      setApiHint("Collez d’abord votre clé.");
      return;
    }
    invoke("set_api_key", { provider: "fish_audio", key })
      .then(() => {
        setHasKey(true);
        setApiKey("");
        setApiHint("Clé enregistrée.");
      })
      .catch(() => setApiHint("Backend en cours : commande set_api_key indisponible."));
  };

  const testApiKey = (): void => {
    invoke<boolean>("has_api_key", { provider: "fish_audio" })
      .then((v) => {
        setHasKey(v === true);
        setApiHint(v === true ? "Clé détectée (stockée côté backend)." : "Aucune clé stockée.");
      })
      .catch(() => setApiHint("Backend en cours : commande has_api_key indisponible."));
  };

  return (
    <div className="settings">
      <aside className="side">
        <div className="brand">
          <img src="/logo.svg" alt="" width={18} height={18} className="brand-logo" aria-hidden="true" />
          <span>whisp</span>
        </div>
        <nav className="tabs" aria-label="Sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`tab${tab === t.id ? " active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <div className="side-mochi">
          <div className="side-mochi-head">
            <Mochi mood="idle" size={48} outfit={outfit} animate={false} />
            <div className="card-head">
              <h2>Mochi</h2>
              <p className="card-hint">Votre compagnon de dictée.</p>
            </div>
          </div>
          <Wardrobe
            value={outfit}
            onChange={(next) => {
              setOutfit(next);
              writeOutfit(next);
            }}
          />
        </div>
        <div className="side-foot">STT local • offline-first</div>
      </aside>

      <main className="content">
        {tab === "home" ? (
          <>
            <section className="panel">
              <div className="card-head">
                <h1>Dicter, c&apos;est écrire</h1>
                <p className="card-hint">Transcription locale, collée à votre curseur.</p>
              </div>
              <p className="muted">
                Appuyez sur votre raccourci global, parlez, et le texte est transcrit localement puis
                collé à votre curseur.
              </p>
              <div className="row">
                <button
                  type="button"
                  className="primary"
                  onClick={() => void safeInvoke("start_recording")}
                >
                  Démarrer l&apos;enregistrement
                </button>
              </div>
              <p className="hints">
                <span>
                  Stop <kbd>Alt + Espace</kbd>
                </span>
                <span>
                  Cancel <kbd>Échap</kbd>
                </span>
              </p>
            </section>
            <section className="panel history">
              <div className="history-head">
                <div className="card-head">
                  <h2>Historique</h2>
                  <p className="card-hint">Vos dernières dictées sur cet appareil.</p>
                </div>
              </div>
              {history.length === 0 ? (
                <div className="empty">
                  <svg className="empty-art" width="48" height="32" viewBox="0 0 48 32" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                    <circle cx="24" cy="16" r="9" />
                    <line x1="24" y1="12" x2="24" y2="16" />
                    <line x1="24" y1="16" x2="27" y2="18" />
                  </svg>
                  <p className="muted">Aucune transcription pour l&apos;instant.</p>
                  <p className="field-hint">Lancez un enregistrement depuis l&apos;onglet Home.</p>
                </div>
              ) : (
                <ul className="history-list">
                  {history.map((h) => (
                    <li key={h.id}>
                      <p>{h.text_final || h.transcript_raw}</p>
                      <div className="meta">
                        <span>{h.mode}</span>
                        <span>{formatHistoryDate(h.timestamp)}</span>
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => deleteEntry(h.id)}
                          aria-label={`Supprimer cette dictée (${h.mode})`}
                        >
                          Supprimer
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </>
        ) : null}

        {tab === "stats" ? (
          <section className="panel">
            <div className="card-head">
              <h1>Statistiques</h1>
              <p className="card-hint">Votre activité de dictée, calculée en local.</p>
            </div>
            <Stats />
          </section>
        ) : null}

        {tab === "chat" ? (
          <section className="panel">
            <div className="card-head chat-head">
              <Mochi mood={chatMood} size={32} outfit={outfit} animate={false} />
              <div>
                <h1>Chat</h1>
                <p className="card-hint">Discutez avec votre modèle local ou cloud.</p>
              </div>
            </div>
            <Chat />
          </section>
        ) : null}

        {tab === "modes" ? (
          <section className="panel">
            <div className="card-head">
              <h1>Modes</h1>
              <p className="card-hint">Un moteur voix-texte par mode de dictée.</p>
            </div>
            <p className="muted">
              Voix-texte par mode. Basculez la voix entre local (hors-ligne) et Fish Audio (nuage,
              rapide) — {hasKey === true ? "clé Fish configurée" : "sans clé Fish, le nuage retombe en local"}.
            </p>
            <ul className="mode-list">
              {(backendModes ?? MODES.map((m) => ({ id: m.toLowerCase(), label: m, preset: "", stt: "local", llm: "", language: "" })) as BackendMode[]).map((m) => (
                <li key={m.id} className="mode-row">
                  <label className="mode-pick">
                    <input
                      type="radio"
                      name="whisp-mode"
                      checked={activeMode.toLowerCase() === m.id}
                      onChange={() => chooseMode(m.label)}
                    />
                    <span>{m.label}</span>
                  </label>
                  <select
                    value={m.stt === "cloud_fish_audio" ? "cloud_fish_audio" : "local"}
                    onChange={(e) => setModeStt(m, e.currentTarget.value as SttKind)}
                    aria-label={`Voix-texte pour ${m.label}`}
                    title={hasKey === true ? "Moteur de transcription" : "Ajoutez votre clé Fish dans Configuration pour activer le nuage"}
                  >
                    <option value="local">Local (hors-ligne)</option>
                    <option value="cloud_fish_audio">Fish Audio (nuage)</option>
                  </select>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        {tab === "vocabulary" ? (
          <section className="panel">
            <div className="card-head">
              <h1>Vocabulaire</h1>
              <p className="card-hint">Mots et noms propres privilégiés à la transcription.</p>
            </div>
            <form
              className="row"
              onSubmit={(e) => {
                e.preventDefault();
                addWord();
              }}
            >
              <input
                value={word}
                onChange={(e) => setWord(e.currentTarget.value)}
                placeholder="Ajouter un mot…"
                aria-label="Ajouter un mot au vocabulaire"
              />
              <button type="submit" className="primary">
                Ajouter
              </button>
            </form>
            <div className="row">
              <button type="button" className="ghost" onClick={applyVocabToActiveMode}>
                Appliquer au mode actif{activeModeEntry ? ` (${activeModeEntry.label})` : ""}
              </button>
              {vocabMsg ? (
                <span className="field-hint" role="status">
                  {vocabMsg}
                </span>
              ) : null}
            </div>
            {vocab.length === 0 ? (
              <div className="empty">
                <svg className="empty-art" width="48" height="32" viewBox="0 0 48 32" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                  <line x1="6" y1="13" x2="6" y2="19" />
                  <line x1="14" y1="9" x2="14" y2="23" />
                  <line x1="22" y1="5" x2="22" y2="27" />
                  <line x1="30" y1="10" x2="30" y2="22" />
                  <line x1="38" y1="14" x2="38" y2="18" />
                </svg>
                <p className="muted">Aucun mot personnalisé pour l&apos;instant.</p>
                <p className="field-hint">Ajoutez un nom propre ci-dessus — il sera privilégié à la transcription.</p>
              </div>
            ) : (
              <ul className="vocab-list">
                {vocab.map((w, i) => (
                  <li key={`${w}-${i}`}>
                    <span>{w}</span>
                    <button type="button" className="ghost" onClick={() => removeWord(i)}>
                      Retirer
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        ) : null}

        {tab === "config" ? (
          <>
            <section className="panel">
              <div className="card-head">
                <h1>Configuration</h1>
                <p className="card-hint">Comportement général de l&apos;application.</p>
              </div>
              <div className="check-row">
                <span id="cfg-launch">Lancer au démarrage</span>
                <Toggle checked={prefs.launchAtLogin} onChange={(v) => patchPrefs({ launchAtLogin: v })} label="Lancer au démarrage" />
              </div>
              <div className="check-row">
                <span id="cfg-sound">Son de fin de transcription</span>
                <Toggle checked={prefs.playSound} onChange={(v) => patchPrefs({ playSound: v })} label="Son de fin de transcription" />
              </div>
              <div className="check-row">
                <span id="cfg-copy">Copier automatiquement dans le presse-papiers</span>
                <Toggle checked={prefs.autoCopy} onChange={(v) => patchPrefs({ autoCopy: v })} label="Copier automatiquement dans le presse-papiers" />
              </div>
              <label className="check-row">
                <span>Langue de dictée</span>
                <select value={dictLang} onChange={(e) => pushDictLang(e.currentTarget.value)}>
                  <option value="fr">Français</option>
                  <option value="en">English</option>
                  <option value="auto">Auto (détection)</option>
                </select>
              </label>
              <label className="check-row">
                <span>Microphone</span>
                <select
                  value={inputDevice}
                  onChange={(e) => pushInputDevice(e.currentTarget.value)}
                  title="Laissez vide pour le micro système par défaut"
                >
                  <option value="">Système (défaut)</option>
                  {inputDevices.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
              <label className="check-row">
                <span>Thème</span>
                <select
                  value={theme}
                  onChange={(e) => pushTheme(e.currentTarget.value as ThemePref)}
                >
                  <option value="system">Système</option>
                  <option value="dark">Sombre</option>
                  <option value="light">Clair</option>
                </select>
              </label>
              <label className="check-row">
                <span>Modèle STT</span>
                <select
                  value={sttModel}
                  onChange={(e) => pushSttModel(e.currentTarget.value as SttModel)}
                >
                  <option value="small">Small — vitesse (défaut)</option>
                  <option value="base">Base — équilibré</option>
                  <option value="medium">Medium — qualité</option>
                </select>
              </label>
              <label className="check-row">
                <span>Position de la popup</span>
                <select
                  value={popupPos}
                  onChange={(e) => pushPopupPos(e.currentTarget.value as PopupPos)}
                >
                  {POPUP_POS_OPTIONS.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.label}
                    </option>
                  ))}
                </select>
              </label>
              <div className="field-label">
                <span>
                  Clé Fish Audio{hasKey === true ? " (configurée)" : hasKey === false ? " (absente)" : ""}
                </span>
                <div className="api-key-row">
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.currentTarget.value)}
                    placeholder="Coller la clé Fish Audio…"
                    aria-label="Clé API Fish Audio"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <button type="button" className="primary" onClick={saveApiKey}>
                    Enregistrer
                  </button>
                  <button type="button" className="ghost" onClick={testApiKey}>
                    Tester
                  </button>
                </div>
                {apiHint ? <span className="field-hint">{apiHint}</span> : null}
              </div>
            </section>
            <section className="panel">
              <div className="card-head">
                <h2>Agent STA</h2>
                <p className="card-hint">Agent lancé avec votre dictée comme demande.</p>
              </div>
              <label className="check-row">
                <span>Agent STA</span>
                <select
                  value={staAgent}
                  onChange={(e) => pushStaAgent(e.currentTarget.value)}
                  title="Agent lancé par le mode STA avec ta dictée comme demande"
                >
                  <option value="omp">omp (défaut)</option>
                  <option value="claude">Claude Code</option>
                  <option value="codex">Codex</option>
                  <option value="custom">Personnalisé…</option>
                </select>
              </label>
              {staAgent === "custom" ? (
                <label className="check-row">
                  <span>Commande STA</span>
                  <input
                    value={staCommand}
                    onChange={(e) => pushStaCommand(e.currentTarget.value)}
                    placeholder="ma-cli --prompt {prompt}"
                    aria-label="Commande STA personnalisée, {prompt} = ta dictée"
                  />
                </label>
              ) : null}
              {staStatus ? (
                <section className="sta-card" aria-label="Session agent STA">
                  <div className="sta-head">
                    <h2 className="sta-title">Session agent</h2>
                    <span className={`sta-badge${staStatus.active ? " on" : ""}`}>
                      {staStatus.active ? "Active" : "Aucune"}
                    </span>
                  </div>
                  <dl className="sta-rows">
                    <div className="sta-row">
                      <dt>Agent</dt>
                      <dd>{staStatus.agent}</dd>
                    </div>
                    <div className="sta-row">
                      <dt>Dossier</dt>
                      <dd title={staStatus.cwd}>{staStatus.cwd}</dd>
                    </div>
                    <div className="sta-row">
                      <dt>Dernier prompt</dt>
                      <dd title={staStatus.lastPrompt ?? undefined}>
                        {staStatus.lastPrompt ? truncate120(staStatus.lastPrompt) : "—"}
                      </dd>
                    </div>
                  </dl>
                  <div className="row sta-actions">
                    <button
                      type="button"
                      className="primary"
                      onClick={openStaTerminal}
                      disabled={staLaunching}
                      aria-busy={staLaunching}
                    >
                      {staLaunching ? (
                        <span className="btn-spinner" aria-hidden="true" />
                      ) : null}
                      {staLaunching ? "Lancement…" : "Ouvrir le terminal agent"}
                    </button>
                    <button type="button" className="ghost" onClick={refreshStaStatus}>
                      Actualiser
                    </button>
                  </div>
                  {staOpenMsg ? <p className="field-hint" role="status">{staOpenMsg}</p> : null}
                </section>
              ) : null}
            </section>
          </>
        ) : null}
      </main>
    </div>
  );
}
