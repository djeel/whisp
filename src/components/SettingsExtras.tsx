import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Interrupteur custom (même langage que App.tsx : 40x22px, 150ms, AA). */
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

// ---------------------------------------------------------------------------
// Voix : voix TTS Fish (reference_id) + Tester + réponse vocale STA.
// Pas de vitesse/timbre : l'API Fish TTS v1 n'expose que `reference_id`.
// ---------------------------------------------------------------------------

export function VoiceSection({ hasKey }: { hasKey: boolean | null }): React.JSX.Element {
  const [voice, setVoice] = useState(() => readStored("whisp:fish-voice") ?? "");
  const [staVoice, setStaVoice] = useState(() => readStored("whisp:sta-voice") === "1");
  const [msg, setMsg] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    invoke<Record<string, unknown>>("get_config")
      .then((cfg) => {
        if (typeof cfg?.fishVoice === "string" && cfg.fishVoice) {
          setVoice(cfg.fishVoice);
          writeStored("whisp:fish-voice", cfg.fishVoice);
        }
        if (typeof cfg?.staVoice === "boolean") {
          setStaVoice(cfg.staVoice);
          writeStored("whisp:sta-voice", cfg.staVoice ? "1" : "0");
        }
      })
      .catch(() => {
        /* backend en cours : localStorage seul */
      });
  }, []);

  const pushVoice = (next: string): void => {
    setVoice(next);
    writeStored("whisp:fish-voice", next);
    invoke("set_config", { fishVoice: next || null }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const pushStaVoice = (next: boolean): void => {
    setStaVoice(next);
    writeStored("whisp:sta-voice", next ? "1" : "0");
    invoke("set_config", { staVoice: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const testVoice = (): void => {
    if (hasKey === false) {
      setMsg("Ajoutez d'abord votre clé Fish dans Configuration.");
      return;
    }
    setTesting(true);
    setMsg(null);
    invoke("speak_text", { text: "Bonjour", voice: voice || null })
      .then(() => setMsg("Test lu à voix haute."))
      .catch((e) => setMsg(`Échec du test : ${String(e)}`))
      .finally(() => setTesting(false));
  };

  return (
    <section className="panel">
      <div className="card-head">
        <h2>Voix</h2>
        <p className="card-hint">Voix Fish Audio pour la lecture et les réponses STA.</p>
      </div>
      <label className="check-row">
        <span>Voix TTS Fish (reference_id)</span>
        <input
          value={voice}
          onChange={(e) => pushVoice(e.currentTarget.value)}
          placeholder="Coller le reference_id fish.audio…"
          aria-label="Voix TTS Fish Audio (reference_id)"
          autoComplete="off"
          spellCheck={false}
        />
      </label>
      <div className="row">
        <button type="button" className="primary" onClick={testVoice} disabled={testing}>
          {testing ? "Lecture…" : "Tester (« Bonjour »)"}
        </button>
        {msg ? (
          <span className="field-hint" role="status">
            {msg}
          </span>
        ) : null}
      </div>
      <div className="check-row">
        <span id="voice-sta">Réponse vocale STA</span>
        <Toggle
          checked={staVoice}
          onChange={(v) => pushStaVoice(v)}
          label="Lire à voix haute les réponses du mode STA"
        />
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Raccourcis : inventaire depuis hotkeys.rs + rechargement Hyprland.
// ---------------------------------------------------------------------------

interface ShortcutInfo {
  id: string;
  label: string;
  keys: string;
  description: string;
}

const FALLBACK_SHORTCUTS: ShortcutInfo[] = [
  { id: "toggle", label: "Dicter / arrêter", keys: "Alt + Espace", description: "Bascule l'enregistrement (global)" },
  { id: "push-to-talk", label: "Push-to-talk", keys: "Ctrl + Alt + Espace (maintenir)", description: "Enregistre tant que les touches sont maintenues" },
  { id: "cycle-mode", label: "Changer de mode", keys: "Ctrl + Alt + M", description: "Cycle les modes de dictée" },
  { id: "cancel", label: "Annuler", keys: "Échap", description: "Annule la dictée en cours (fenêtre popup)" },
];

export function ShortcutsSection(): React.JSX.Element {
  const [shortcuts, setShortcuts] = useState<ShortcutInfo[]>(FALLBACK_SHORTCUTS);
  const [msg, setMsg] = useState<string | null>(null);
  const [reloading, setReloading] = useState(false);

  useEffect(() => {
    invoke<ShortcutInfo[]>("list_shortcuts")
      .then((rows) => {
        if (Array.isArray(rows) && rows.length > 0) setShortcuts(rows);
      })
      .catch(() => {
        /* backend en cours : liste statique */
      });
  }, []);

  const reload = (): void => {
    setReloading(true);
    setMsg(null);
    invoke<string>("reload_hyprland")
      .then((m) => setMsg(m))
      .catch((e) => setMsg(`Échec : ${String(e)}`))
      .finally(() => setReloading(false));
  };

  return (
    <section className="panel">
      <div className="card-head">
        <h2>Raccourcis</h2>
        <p className="card-hint">Tous les raccourcis whisp, puis recharge Hyprland.</p>
      </div>
      <ul className="shortcut-list">
        {shortcuts.map((s) => (
          <li key={s.id} className="row shortcut">
            <span className="shortcut-name" title={s.description}>
              {s.label}
            </span>
            <span className="keycap static">{s.keys}</span>
          </li>
        ))}
      </ul>
      <div className="row">
        <button type="button" className="ghost" onClick={reload} disabled={reloading}>
          {reloading ? "Rechargement…" : "Recharger Hyprland"}
        </button>
        {msg ? (
          <span className="field-hint" role="status">
            {msg}
          </span>
        ) : null}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Apparence : taille popup S/M/L, opacité carte, Mochi dans la popup.
// ---------------------------------------------------------------------------

type PopupSize = "S" | "M" | "L";

function isPopupSize(v: unknown): v is PopupSize {
  return v === "S" || v === "M" || v === "L";
}

export function AppearanceSection({
  theme,
  onTheme,
  popupPos,
  onPopupPos,
}: {
  theme: string;
  onTheme: (v: "system" | "dark" | "light") => void;
  popupPos: string;
  onPopupPos: (v: "top-center" | "center" | "bottom-center" | "cursor") => void;
}): React.JSX.Element {
  const [size, setSize] = useState<PopupSize>(() => {
    const raw = readStored("whisp:popup-size");
    return isPopupSize(raw) ? raw : "M";
  });
  const [opacity, setOpacity] = useState<number>(() => {
    const raw = Number(readStored("whisp:card-opacity"));
    return Number.isFinite(raw) && raw >= 0.4 && raw <= 1 ? raw : 1;
  });
  const [mochi, setMochi] = useState(() => readStored("whisp:popup-mochi") !== "0");
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    invoke<Record<string, unknown>>("get_config")
      .then((cfg) => {
        if (isPopupSize(cfg?.popupSize)) {
          setSize(cfg.popupSize);
          writeStored("whisp:popup-size", cfg.popupSize);
        }
        if (typeof cfg?.cardOpacity === "number" && cfg.cardOpacity >= 0.4 && cfg.cardOpacity <= 1) {
          setOpacity(cfg.cardOpacity);
          writeStored("whisp:card-opacity", String(cfg.cardOpacity));
        }
        if (typeof cfg?.popupMochi === "boolean") {
          setMochi(cfg.popupMochi);
          writeStored("whisp:popup-mochi", cfg.popupMochi ? "1" : "0");
        }
      })
      .catch(() => {
        /* backend en cours : localStorage seul */
      });
    document.documentElement.style.setProperty(
      "--card-alpha",
      String(Number(readStored("whisp:card-opacity") ?? "1") || 1),
    );
  }, []);

  const pushSize = (next: PopupSize): void => {
    setSize(next);
    writeStored("whisp:popup-size", next);
    setMsg(null);
    invoke<string>("set_popup_size", { size: next })
      .then((m) => setMsg(m))
      .catch(() => setMsg("Taille mémorisée (popup injoignable)."));
    invoke("set_config", { popupSize: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const pushOpacity = (next: number): void => {
    setOpacity(next);
    writeStored("whisp:card-opacity", String(next));
    document.documentElement.style.setProperty("--card-alpha", String(next));
    invoke("set_config", { cardOpacity: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  const pushMochi = (next: boolean): void => {
    setMochi(next);
    writeStored("whisp:popup-mochi", next ? "1" : "0");
    invoke("set_config", { popupMochi: next }).catch(() => {
      /* backend en cours : localStorage seul */
    });
  };

  return (
    <section className="panel">
      <div className="card-head">
        <h2>Apparence</h2>
        <p className="card-hint">Thème, popup et carte d'enregistrement.</p>
      </div>
      <label className="check-row">
        <span>Thème</span>
        <select
          value={theme}
          onChange={(e) => onTheme(e.currentTarget.value as "system" | "dark" | "light")}
        >
          <option value="system">Système</option>
          <option value="dark">Sombre</option>
          <option value="light">Clair</option>
        </select>
      </label>
      <label className="check-row">
        <span>Position de la popup</span>
        <select
          value={popupPos}
          onChange={(e) =>
            onPopupPos(e.currentTarget.value as "top-center" | "center" | "bottom-center" | "cursor")
          }
        >
          <option value="top-center">Haut-centre (Superwhisper)</option>
          <option value="center">Centre</option>
          <option value="bottom-center">Bas-centre</option>
          <option value="cursor">Près du curseur</option>
        </select>
      </label>
      <label className="check-row">
        <span>Taille de la popup</span>
        <select value={size} onChange={(e) => pushSize(e.currentTarget.value as PopupSize)}>
          <option value="S">S — 640px</option>
          <option value="M">M — 720px</option>
          <option value="L">L — 880px</option>
        </select>
      </label>
      <label className="check-row">
        <span>Opacité de la carte ({Math.round(opacity * 100)} %)</span>
        <input
          type="range"
          min={0.4}
          max={1}
          step={0.05}
          value={opacity}
          onChange={(e) => pushOpacity(Number(e.currentTarget.value))}
          aria-label="Opacité de la carte popup"
        />
      </label>
      <div className="check-row">
        <span id="appearance-mochi">Mochi dans la popup</span>
        <Toggle checked={mochi} onChange={(v) => pushMochi(v)} label="Afficher Mochi dans la popup" />
      </div>
      {msg ? (
        <span className="field-hint" role="status">
          {msg}
        </span>
      ) : null}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Mises à jour : version locale + comparaison GitHub djeel/whisp.
// ---------------------------------------------------------------------------

interface UpdateInfo {
  current: string;
  latest?: string | null;
  updateAvailable: boolean;
  url?: string | null;
  message?: string | null;
}

export function UpdatesSection(): React.JSX.Element {
  const [version, setVersion] = useState<string | null>(null);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    invoke<string>("app_version")
      .then((v) => setVersion(v))
      .catch(() => setVersion(null));
  }, []);

  const check = (): void => {
    setChecking(true);
    invoke<UpdateInfo>("check_updates")
      .then((r) => {
        setInfo(r);
        if (r.current) setVersion(r.current);
      })
      .catch((e) => setInfo({ current: version ?? "?", updateAvailable: false, message: String(e) }))
      .finally(() => setChecking(false));
  };

  const openRelease = (url: string): void => {
    openUrl(url).catch(() => {
      window.open(url, "_blank", "noopener");
    });
  };

  return (
    <section className="panel">
      <div className="card-head">
        <h2>Mises à jour</h2>
        <p className="card-hint">Version installée et dernière release GitHub.</p>
      </div>
      <div className="check-row">
        <span>Version installée</span>
        <span className="field-hint">{version ?? "…"}</span>
      </div>
      <div className="row">
        <button type="button" className="ghost" onClick={check} disabled={checking}>
          {checking ? "Vérification…" : "Vérifier"}
        </button>
        {info?.message ? (
          <span className="field-hint" role="status">
            {info.updateAvailable && info.latest
              ? `Nouvelle version ${info.latest} disponible.`
              : info.message}
          </span>
        ) : null}
      </div>
      {info?.updateAvailable && info.url ? (
        <div className="row">
          <button type="button" className="primary" onClick={() => openRelease(info.url as string)}>
            Télécharger {info.latest ?? "la mise à jour"}
          </button>
        </div>
      ) : null}
    </section>
  );
}
