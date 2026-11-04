import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ChatProvider =
  | "ollama"
  | "anthropic"
  | "openai"
  | "google"
  | "openrouter"
  | "deepseek"
  | "qwen"
  | "moonshot";

interface ChatMsg {
  role: "user" | "assistant";
  content: string;
}

const PROVIDERS: Array<{ id: ChatProvider; label: string }> = [
  { id: "ollama", label: "Ollama (local)" },
  { id: "anthropic", label: "Anthropic" },
  { id: "openai", label: "OpenAI" },
  { id: "google", label: "Gemini (OpenAI-compatible)" },
  { id: "openrouter", label: "OpenRouter" },
  { id: "deepseek", label: "DeepSeek" },
  { id: "qwen", label: "Qwen" },
  { id: "moonshot", label: "Moonshot" },
];

function loadHistory(): ChatMsg[] {
  try {
    const raw = localStorage.getItem("whisp:chat-history");
    if (!raw) return [];
    const v: unknown = JSON.parse(raw);
    if (!Array.isArray(v)) return [];
    const out: ChatMsg[] = [];
    for (const item of v) {
      if (typeof item !== "object" || item === null) continue;
      const role = "role" in item ? item.role : undefined;
      const content = "content" in item ? item.content : undefined;
      if ((role === "user" || role === "assistant") && typeof content === "string") {
        out.push({ role, content });
      }
    }
    return out.slice(-50);
  } catch {
    return [];
  }
}

export default function Chat() {
  const [provider, setProvider] = useState<ChatProvider>(() => {
    try {
      const v = localStorage.getItem("whisp:chat-provider");
      return (
        ["ollama", "anthropic", "openai", "google", "openrouter", "deepseek", "qwen", "moonshot"] as ChatProvider[]
      ).includes(v as ChatProvider)
        ? (v as ChatProvider)
        : "ollama";
    } catch {
      return "ollama";
    }
  });
  const [messages, setMessages] = useState<ChatMsg[]>(loadHistory);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [keyHint, setKeyHint] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem("whisp:chat-history", JSON.stringify(messages.slice(-50)));
    } catch {
      /* storage unavailable */
    }
  }, [messages]);

  useEffect(() => {
    try {
      localStorage.setItem("whisp:chat-provider", provider);
    } catch {
      /* storage unavailable */
    }
  }, [provider]);

  useEffect(() => {
    if (provider === "ollama") {
      setHasKey(null);
      return;
    }
    invoke<boolean>("has_api_key", { provider })
      .then((v) => setHasKey(v === true))
      .catch(() => setHasKey(null));
  }, [provider]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages, busy]);

  const needsKey = provider !== "ollama" && hasKey === false;

  const saveKey = (): void => {
    const key = keyInput.trim();
    if (!key) {
      setKeyHint("Collez d'abord votre clé.");
      return;
    }
    invoke("set_api_key", { provider, key })
      .then(() => {
        setHasKey(true);
        setKeyInput("");
        setKeyHint("Clé enregistrée.");
      })
      .catch(() => setKeyHint("Backend en cours : commande set_api_key indisponible."));
  };

  const send = (): void => {
    const text = draft.trim();
    if (!text || busy) return;
    const next: ChatMsg[] = [...messages, { role: "user" as const, content: text }].slice(-50);
    setMessages(next);
    setDraft("");
    setBusy(true);
    setError(null);
    invoke<string>("chat_command", { messages: next.slice(-20), provider })
      .then((reply) => {
        setMessages((prev) => [...prev, { role: "assistant" as const, content: reply || "…" }].slice(-50));
      })
      .catch((e: unknown) => {
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => setBusy(false));
  };

  return (
    <div className="chat">
      <div className="row chat-bar">
        <label className="check-row">
          <span>Fournisseur</span>
          <select value={provider} onChange={(e) => setProvider(e.target.value as ChatProvider)} aria-label="Fournisseur de chat">
            {PROVIDERS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
        {provider !== "ollama" ? (
          <span className="field-hint">{hasKey === true ? "Clé configurée." : hasKey === false ? "Clé manquante." : "État de la clé inconnu."}</span>
        ) : (
          <span className="field-hint">Local, sans clé (qwen2.5:3b).</span>
        )}
      </div>
      {needsKey ? (
        <div className="row chat-key">
          <input
            type="password"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            placeholder={`Coller la clé ${provider}…`}
            aria-label={`Clé API ${provider}`}
            autoComplete="off"
            spellCheck={false}
          />
          <button type="button" className="primary" onClick={saveKey}>
            Enregistrer
          </button>
          {keyHint ? <span className="field-hint">{keyHint}</span> : null}
        </div>
      ) : null}
      <div className="chat-log" role="log" aria-label="Conversation">
        {messages.length === 0 ? (
          <p className="muted">Posez une question pour démarrer la conversation.</p>
        ) : (
          messages.map((m, i) => (
            <div key={i} className={`chat-msg ${m.role}`}>
              <span className="chat-who">{m.role === "user" ? "Vous" : "Assistant"}</span>
              <p>{m.content}</p>
            </div>
          ))
        )}
        {busy ? (
          <div className="chat-msg assistant pending" aria-live="polite">
            <span className="chat-who">Assistant</span>
            <p>
              <span className="dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>{" "}
              Réponse en cours…
            </p>
          </div>
        ) : null}
        <div ref={bottomRef} />
      </div>
      {error ? (
        <p className="field-hint" role="alert">
          {error}
        </p>
      ) : null}
      <form
        className="row chat-input"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Écrivez votre message…"
          aria-label="Message à envoyer"
          disabled={busy}
        />
        <button type="submit" className="primary" disabled={busy || !draft.trim()}>
          {busy ? "…" : "Envoyer"}
        </button>
      </form>
    </div>
  );
}
