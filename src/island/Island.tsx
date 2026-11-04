import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import Mochi from "../components/Mochi";
import { moodFromState } from "../mochi/types";

/// Keep the 64 px-tall island window fitted to its content: collapsed stays
/// 64 px, expanded grows with the card list (clamped 64..400). No-ops in
/// browser dev (no Tauri runtime). The window must follow the DOM or
/// approvals/buttons clip (see Top-10 #8).
function fitIslandWindow(expanded: boolean): void {
  if (!isTauri()) return;
  // Measure after the DOM committed this frame.
  requestAnimationFrame(() => {
    const h = document.documentElement?.scrollHeight ?? 64;
    const target = expanded ? Math.min(400, Math.max(64, h)) : 64;
    getCurrentWindow()
      .setSize(new LogicalSize(720, target))
      .catch(() => undefined);
  });
}

export type PillState = "idle" | "working" | "thinking" | "finished" | "error";

export interface Pill {
  session_id: string;
  agent: string;
  state: PillState;
  last_line: string;
  cwd?: string | null;
  pending_approval: boolean;
  updated_at: number;
}

export interface ApprovalQuestion {
  question: string;
  options: string[];
  multi: boolean;
}

export interface ApprovalCard {
  request_id: string;
  session_id: string;
  agent: string;
  tool: string;
  command: string;
  questions?: ApprovalQuestion[] | null;
}

type IslandView = "hidden" | "petit" | "home";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function stateOf(pills: Pill[]): IslandView {
  if (pills.length === 0) return "hidden";
  const active = pills.some(
    (p) => p.state === "working" || p.state === "thinking" || p.pending_approval,
  );
  return active ? "home" : "petit";
}

const DOT: Record<PillState, string> = {
  idle: "var(--muted)",
  working: "var(--green)",
  thinking: "#f5c542",
  finished: "#58a6ff",
  error: "var(--red)",
};

export default function Island() {
  const [pills, setPills] = useState<Pill[]>([]);
  const [approvals, setApprovals] = useState<ApprovalCard[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<IslandView>("hidden");

  // Initial snapshots (backend is the source of truth).
  useEffect(() => {
    if (!isTauri()) return;
    invoke<Pill[]>("island_list")
      .then((list) => {
        if (Array.isArray(list)) setPills(list);
      })
      .catch(() => {
        /* backend starting */
      });
    invoke<ApprovalCard[]>("approval_list")
      .then((list) => {
        if (Array.isArray(list)) setApprovals(list);
      })
      .catch(() => {
        /* backend starting */
      });
  }, []);

  // Live updates from the backend. Canonical snapshot event carries the full
  // pill list; a raw normalized payload (older relay) just triggers a pull.
  useEffect(() => {
    let cancelled = false;
    let offSnap: (() => void) | undefined;
    let offRaw: (() => void) | undefined;
    const pull = (): void => {
      void invoke<Pill[]>("island_list")
        .then((list) => {
          if (!cancelled && Array.isArray(list)) setPills(list);
        })
        .catch(() => undefined);
      void invoke<ApprovalCard[]>("approval_list")
        .then((list) => {
          if (!cancelled && Array.isArray(list)) setApprovals(list);
        })
        .catch(() => undefined);
    };
    void (async () => {
      try {
        offSnap = await listen<unknown>("whisp://agent", (e) => {
          if (cancelled) return;
          const payload = e.payload;
          if (payload && typeof payload === "object" && "pills" in payload) {
            const list: unknown = payload.pills;
            if (Array.isArray(list)) setPills(list as Pill[]);
          }
          if (payload && typeof payload === "object" && "approvals" in payload) {
            const list: unknown = payload.approvals;
            if (Array.isArray(list)) setApprovals(list as ApprovalCard[]);
          }
        });
        offRaw = await listen<unknown>("whisp://agent-raw", () => {
          if (!cancelled) pull();
        });
      } catch {
        /* browser dev without Tauri runtime */
      }
    })();
    return () => {
      cancelled = true;
      if (offSnap) offSnap();
      if (offRaw) offRaw();
    };
  }, []);

  // FSM: hidden | petit | home; activity shows the island, 60 s idle hides it.
  const [lastActive, setLastActive] = useState(0);
  useEffect(() => {
    const v = approvals.length > 0 ? "home" : stateOf(pills);
    setExpanded((x) => (approvals.length > 0 ? true : x));
    setView(v);
    if (v !== "hidden") setLastActive(Date.now());
  }, [pills, approvals]);
  useEffect(() => {
    if (view === "hidden") return;
    const id = window.setTimeout(() => {
      if (Date.now() - lastActive >= 60_000) {
        setView("hidden");
        setExpanded(false);
        if (isTauri()) {
          void invoke("island_dismiss").catch(() => undefined);
        }
      }
    }, 5_000);
    return () => window.clearTimeout(id);
  }, [view, lastActive]);

  // After every content change, fit the window to the card (petit → 64,
  // expand/home → measured height clamped 64..400).
  useEffect(() => {
    if (view === "hidden") return;
    fitIslandWindow(expanded);
  }, [view, expanded, pills, approvals]);

  const toggle = useCallback(() => setExpanded((x) => !x), []);

  // Selection follows the pill list: default to the freshest pill, drop the
  // selection when its pill disappears.
  useEffect(() => {
    if (pills.length === 0) {
      setSelected(null);
      return;
    }
    setSelected((prev) =>
      prev !== null && pills.some((p) => p.session_id === prev) ? prev : pills[0].session_id,
    );
  }, [pills]);

  if (view === "hidden" && pills.length === 0 && !isTauri()) {
    // Browser preview: render demo content so `?window=island` is reviewable.
    return <IslandCard pills={demoPills()} expanded={expanded} onToggle={toggle} preview />;
  }
  if (view === "hidden") return null;
  return (
    <IslandCard
      pills={pills}
      approvals={approvals}
      expanded={expanded}
      onToggle={toggle}
      compact={view === "petit"}
      selected={selected}
      onSelect={setSelected}
    />
  );
}

function IslandCard({
  pills,
  approvals = [],
  expanded,
  onToggle,
  compact,
  preview,
  selected,
  onSelect,
}: {
  pills: Pill[];
  approvals?: ApprovalCard[];
  expanded: boolean;
  onToggle: () => void;
  compact?: boolean;
  preview?: boolean;
  selected?: string | null;
  onSelect?: (sessionId: string) => void;
}) {
  const first = pills[0];
  const pendingCount = approvals.length;
  return (
    <div className="island-stage">
      <div
        className={`island-card${expanded ? " expanded" : ""}${compact && !expanded ? " compact" : ""}`}
        role="dialog"
        aria-label="whisp agents"
      >
        <button type="button" className="island-bar" onClick={onToggle} aria-expanded={expanded}>
          <Mochi
            mood={first ? moodFromState(first.state, first.pending_approval) : "idle"}
            size={28}
          />
          {first ? (
            <>
              <span
                className="island-dot"
                style={{ background: DOT[first.state] ?? DOT.idle }}
                aria-hidden="true"
              />
              <span className="island-agent">{first.agent}</span>
              <span className="island-state">{first.state}</span>
              <span className="island-ticker">{first.last_line || "—"}</span>
              {pills.length > 1 ? <span className="island-count">+{pills.length - 1}</span> : null}
              {pendingCount > 0 ? (
                <span className="island-ask" aria-label={`${pendingCount} pending approvals`}>
                  {pendingCount}
                </span>
              ) : null}
            </>
          ) : (
            <span className="island-ticker">no agents</span>
          )}
          {preview ? <span className="island-count">preview</span> : null}
        </button>
        {expanded ? (
          <div className="island-list">
            {approvals.map((a) => (
              <ApprovalView key={a.request_id} card={a} />
            ))}
            {pills.map((p) => (
              <div key={`${p.agent}:${p.session_id}`}>
                <div
                  className={`island-pill${selected === p.session_id ? " selected" : ""}`}
                  role="button"
                  tabIndex={0}
                  aria-pressed={selected === p.session_id}
                  onClick={() => onSelect?.(p.session_id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelect?.(p.session_id);
                    }
                  }}
                >
                  <span
                    className="island-dot"
                    style={{ background: DOT[p.state] ?? DOT.idle }}
                    aria-hidden="true"
                  />
                  <span className="island-agent">{p.agent}</span>
                  <span className="island-state">{p.state}</span>
                  <span className="island-ticker">{p.last_line || "—"}</span>
                  {p.pending_approval ? <span className="island-ask">●</span> : null}
                </div>
                {selected === p.session_id ? <MiniTerm sessionId={p.session_id} /> : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/// One permission request waiting for a human click. Approval cards get
/// Allow / Deny / Toujours; question cards get one button per option
/// (multi-select toggles, then an Answer button). Click → `agent_decide`.
/// Never auto-clicks: no timers, no defaults.
function ApprovalView({ card }: { card: ApprovalCard }) {
  const [busy, setBusy] = useState(false);
  const [gone, setGone] = useState(false);
  const [picks, setPicks] = useState<Record<string, string | string[]>>({});
  const questions = card.questions ?? [];
  const isQuestion = questions.length > 0;

  const decide = useCallback(
    (decision: string, answers?: Record<string, string | string[]>) => {
      if (busy || !isTauri()) return;
      setBusy(true);
      invoke("agent_decide", {
        requestId: card.request_id,
        decision,
        answers: answers ?? null,
      })
        .then(() => setGone(true))
        .catch(() => setBusy(false));
    },
    [busy, card.request_id],
  );

  if (gone) return null;
  const ready =
    !isQuestion ||
    questions.every((q) => {
      const a: unknown = picks[q.question];
      return q.multi ? Array.isArray(a) && (a as string[]).length > 0 : typeof a === "string";
    });

  return (
    <div className="island-approval" role="group" aria-label={`${card.agent} wants approval`}>
      <div className="island-pill">
        <span className="island-dot" style={{ background: "#f5c542" }} aria-hidden="true" />
        <span className="island-agent">{card.agent}</span>
        <span className="island-state">{isQuestion ? "question" : "approval"}</span>
        <span className="island-ticker">
          {card.tool}
          {card.command ? ` · ${card.command}` : ""}
        </span>
      </div>
      {isQuestion ? (
        <div className="island-questions">
          {questions.map((q) => {
            const a: unknown = picks[q.question];
            return (
              <div key={q.question} className="island-question">
                <div className="island-question-text">{q.question}</div>
                <div className="island-actions">
                  {q.options.map((label) => {
                    const on = q.multi
                      ? Array.isArray(a) && (a as string[]).includes(label)
                      : a === label;
                    return (
                      <button
                        key={label}
                        type="button"
                        className="rec-btn"
                        disabled={busy}
                        aria-pressed={on}
                        onClick={() => {
                          if (q.multi) {
                            setPicks((prev) => {
                              const cur = Array.isArray(prev[q.question])
                                ? (prev[q.question] as string[])
                                : [];
                              const next = cur.includes(label)
                                ? cur.filter((x) => x !== label)
                                : [...cur, label];
                              return { ...prev, [q.question]: next };
                            });
                          } else {
                            setPicks((prev) => ({ ...prev, [q.question]: label }));
                          }
                        }}
                      >
                        {on ? "✓ " : ""}
                        {label}
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
          <div className="island-actions">
            <button
              type="button"
              className="rec-btn"
              disabled={busy || !ready}
              onClick={() => decide("question", picks)}
            >
              Answer
            </button>
            <button type="button" className="rec-btn" disabled={busy} onClick={() => decide("deny")}>
              Deny
            </button>
          </div>
        </div>
      ) : (
        <div className="island-actions">
          <button type="button" className="rec-btn" disabled={busy} onClick={() => decide("allow")}>
            Allow
          </button>
          <button type="button" className="rec-btn" disabled={busy} onClick={() => decide("deny")}>
            Deny
          </button>
          <button type="button" className="rec-btn" disabled={busy} onClick={() => decide("always")}>
            Toujours
          </button>
        </div>
      )}
    </div>
  );
}

/// Mini-terminal for one pill: last tmux lines, 1 s poll, input via send-keys.
/// No xterm.js: a <pre> + form keeps the bundle unchanged and never blocks the
/// agent (poll stops when collapsed or when no backend answers).
function MiniTerm({ sessionId }: { sessionId: string }) {
  const [text, setText] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    const pull = (): void => {
      void invoke<string>("island_term_view", { sessionId })
        .then((t) => {
          if (!cancelled) setText(t);
        })
        .catch(() => {
          if (!cancelled) setText(null);
        });
    };
    pull();
    const id = window.setInterval(pull, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [sessionId]);

  const send = (): void => {
    const t = input.trim();
    if (!t || sending || !isTauri()) return;
    setSending(true);
    void invoke("island_term_send", { sessionId, text: t })
      .then(() => {
        setInput("");
      })
      .catch(() => undefined)
      .finally(() => {
        setSending(false);
      });
  };

  return (
    <div className="island-term">
      {text !== null ? (
        <pre className="island-term-view">{text || "(empty)"}</pre>
      ) : (
        <p className="island-term-empty">no live terminal for this session</p>
      )}
      <form
        className="island-term-form"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <input
          type="text"
          className="island-term-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Type an instruction…"
          aria-label="Send to session"
          maxLength={4000}
        />
        <button type="submit" className="rec-btn" disabled={!input.trim() || sending}>
          Send
        </button>
      </form>
    </div>
  );
}

function demoPills(): Pill[] {
  return [
    {
      session_id: "sta-omp",
      agent: "omp",
      state: "working",
      last_line: "Editing src/main.rs…",
      cwd: "~/Projets/whisp",
      pending_approval: false,
      updated_at: 0,
    },
    {
      session_id: "demo",
      agent: "demo",
      state: "thinking",
      last_line: "Running task…",
      pending_approval: true,
      updated_at: 0,
    },
  ];
}
