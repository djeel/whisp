<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    currentMonitor,
    getCurrentWindow,
    LogicalPosition,
    LogicalSize,
  } from "@tauri-apps/api/window";
  import { playCue } from "../lib/sound";
  import { readPref } from "../lib/prefs";
  import {
    applyAppearance,
    fetchCaelestiaScheme,
    readAccentCustom,
    readAccentSource,
    startCaelestiaWatch,
    type CaelestiaScheme,
  } from "../lib/accent";
  import Mochi from "../components/Mochi.svelte";
  import Waveform from "../components/Waveform.svelte";
  import ApprovalCard from "../components/ApprovalCard.svelte";
  import {
    EXPAND_ICON,
    isTauri,
    MIC_ICON,
    parseLevel,
    parseState,
    popupWidth,
    savePopupXY,
    SHRINK_ICON,
    applyPopupAppearance,
    applyPopupPosition,
    type Approval,
    type RecorderStatus,
  } from "../lib/recorder";

  const BACKEND_MODES = ["Message", "Mail", "Voice", "Agent"];

  let status: RecorderStatus = $state("recording");
  let mode = $state("Auto");
  let modeIndex = $state(0);
  let message = $state<string | null>(null);
  let mini = $state(false);
  let showMochi = $state(false);
  let partial = $state<string | null>(null);
  void partial;
  let voiceQuestion = $state<string | null>(null);
  let agentActive = $state(false);
  let agentName = $state("omp");
  let agentCwd = $state<string | null>(null);
  let agentStopping = $state(false);
  let agentOutput = $state<string | null>(null);
  let approvals: Approval[] = $state([]);

  const level = { current: 0 };
  let cardEl = $state<HTMLDivElement | null>(null);
  let statusCur: RecorderStatus = "recording";
  const isAgent = $derived(mode.toLowerCase() === "agent");
  // Basename du dossier cible agent (hint état vide) : `~/Projets/foo/` → `foo`.
  const agentCwdBase = $derived(
    agentCwd ? agentCwd.replace(/\/+$/, "").split("/").pop() || agentCwd : null,
  );
  // Dernier setSize commité : garde anti-boucle (pas de fit si Δ < 2 px).
  let lastSize: { w: number; h: number } | null = null;
  const cardClass = $derived(`recorder-card${mini ? " mini" : ""} is-${status}`);
  const mochiMood = $derived(
    status === "recording"
      ? "working"
      : status === "processing"
        ? "thinking"
        : status === "done"
          ? "finished"
          : status === "error"
            ? "error"
            : "idle",
  );

  /** Thème + accent recorder : même résolution que l'app (theme → accent). */
  function applyRecorderAppearance(scheme: CaelestiaScheme | null): void {
    applyAppearance(readPref("whisp:theme"), readAccentSource(), readAccentCustom(), scheme);
  }

  // Autosize : ResizeObserver sur la carte (pattern éprouvé, pas de boucle).
  // - commit via rAF (jamais synchrone : casse la boucle mesure → resize) ;
  // - garde Δ ≥ 1 px (pas de setSize si identique) ;
  // - position recalculée DE LA TAILLE RÉELLE, jamais win.center().
  // Zéro dépendance Hyprland : marche sur tout compositor Wayland/X11.
  $effect(() => {
    void mini;
    void mode;
    void status;
    void message;
    void voiceQuestion;
    void approvals.length;
    void agentActive;
    // PAS agentOutput : change chaque seconde en mode agent (poll 1 Hz) et
    // recréerait le ResizeObserver + relancerait setSize/fit_window en boucle.
    // La sortie agent scrolle dans son <pre> (overflow-y), pas besoin de resize.
    void isAgent;
    if (!isTauri()) return;
    const card = cardEl;
    if (!card) return;
    let frame = 0;
    let pending: { w: number; h: number } | null = null;
    const commit = (): void => {
      frame = 0;
      const next = pending;
      pending = null;
      if (!next) return;
      const last = lastSize;
      // Garde 2 px : ignore le bruit, commit tout vrai changement.
      if (last && Math.abs(last.w - next.w) < 2 && Math.abs(last.h - next.h) < 2) return;
      lastSize = next;
      const want = { w: Math.round(next.w), h: Math.round(next.h) };
      const attempt = (left: number): void => {
        void (async () => {
          const win = getCurrentWindow();
          const cur = await win.innerSize();
          if (Math.abs(cur.width - want.w) < 2 && Math.abs(cur.height - want.h) < 2) return;
          await win.setSize(new LogicalSize(want.w, want.h));
          // Vérifie : si le compositor a ignoré, fallback backend + retry.
          const after = await win.innerSize();
          if (Math.abs(after.width - want.w) > 4 || Math.abs(after.height - want.h) > 4) {
            const r = await invoke<string>("fit_window", want).catch(() => "err");
            const ok = typeof r === "string" && r.startsWith("resized");
            if (!ok && left > 0) window.setTimeout(() => attempt(left - 1), 400);
            else if (!ok) lastSize = null;
          }
        })().catch(() => {
          if (left > 0) window.setTimeout(() => attempt(left - 1), 400);
          else lastSize = null;
        });
      };
      attempt(5);
    };
    const ro = new ResizeObserver(() => {
      const rectH = card.getBoundingClientRect().height;
      const clone = card.cloneNode(true) as HTMLElement;
      clone.style.cssText = `position:fixed;left:-9999px;top:0;width:${card.clientWidth}px;height:auto;max-height:none;visibility:hidden;pointer-events:none;`;
      document.body.appendChild(clone);
      const cloneH = clone.getBoundingClientRect().height;
      clone.remove();
      const real = Math.max(card.scrollHeight, rectH, cloneH, document.documentElement.scrollHeight);
      pending = { w: popupWidth(mini), h: Math.max(64, Math.ceil(real) + 8) };
      if (!frame) frame = requestAnimationFrame(commit);
    });
    ro.observe(card);
    // Re-mesure différée : le DOM peint le module agent/voice APRÈS le commit
    // Svelte. Le RO tire sur l'ancien DOM (200 px) puis se tait. On re-ping
    // à 100/300 ms pour mesurer le vrai contenu une fois peint.
    const t1 = window.setTimeout(() => ro.observe(card), 100);
    const t2 = window.setTimeout(() => ro.observe(card), 300);
    return () => {
      ro.disconnect();
      window.clearTimeout(t1);
      window.clearTimeout(t2);
      if (frame) cancelAnimationFrame(frame);
    };
  });

  // Agent : statut + sortie live, poll seulement en mode agent.
  $effect(() => {
    const agent = isAgent;
    if (!isTauri() || !agent) {
      agentActive = false;
      agentCwd = null;
      agentOutput = null;
      return;
    }
    let cancelled = false;
    const pullStatus = (): void => {
      void invoke<{ active: boolean; agent: string; cwd: string }>("agent_status")
        .then((s) => {
          if (cancelled) return;
          agentActive = s.active === true;
          if (typeof s.agent === "string" && s.agent) agentName = s.agent;
          agentCwd = typeof s.cwd === "string" && s.cwd ? s.cwd : null;
        })
        .catch(() => undefined);
    };
    const pullOutput = (): void => {
      void invoke<string>("agent_last_output")
        .then((t) => {
          if (!cancelled) agentOutput = agentOutput === t ? agentOutput : t;
        })
        .catch(() => {
          if (!cancelled) agentOutput = null;
        });
    };
    pullStatus();
    pullOutput();
    // 1 s : le terminal suit en quasi temps réel (3 s = impression de freeze).
    const id = window.setInterval(() => {
      pullStatus();
      pullOutput();
    }, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  });

  $effect(() => {
    if (!isTauri()) mode = BACKEND_MODES[modeIndex] ?? "Message";
  });

  function stop(): void {
    void invoke("stop_recording").catch(() => {
      /* browser dev without backend */
    });
  }

  function cancel(): void {
    void invoke("cancel_recording").catch(() => {
      /* browser dev without backend */
    });
  }

  function cycleMode(): void {
    if (!isTauri()) {
      modeIndex = (modeIndex + 1) % BACKEND_MODES.length;
      return;
    }
    void invoke<{ id: string; label: string }>("cycle_mode")
      .then((m) => {
        if (m?.label) mode = m.label;
        else if (m?.id) mode = m.id;
      })
      .catch(() => {
        /* backend en cours */
      });
  }

  function toggleSize(): void {
    mini = !mini;
  }

  function decide(requestId: string, decision: string): void {
    approvals = approvals.filter((a) => a.request_id !== requestId);
    void invoke("agent_decide", { requestId, decision }).catch(() => {
      message = `Échec ${decision} (${requestId.slice(0, 8)}…) — réessaie via le terminal.`;
    });
  }

  function stopAgent(): void {
    agentStopping = true;
    void invoke("agent_cancel")
      .catch(() => undefined)
      .finally(() => {
        agentStopping = false;
      });
  }

  function openAgentTerm(): void {
    message = "Ouverture du terminal…";
    void invoke("agent_open_terminal")
      .then(() => {
        message = null;
      })
      .catch((e) => {
        message = `Terminal : ${e instanceof Error ? e.message : String(e)}`;
      });
  }

  // Drag-to-move robuste : natif d'abord (compositor), manuel vérifié
  // en fallback. L'ancien setPosition aveugle était ignoré par Wayland
  // (popup jamais déplacée) ; startDragging seul échoue silencieusement
  // hors Hyprland. Ici chaque étape est vérifiée.
  function onDragStart(e: MouseEvent): void {
    if (e.button !== 0 || !isTauri()) return;
    const win = getCurrentWindow();
    void (async () => {
      // 1. Snapshot avant : pour détecter si le natif a vraiment bougé.
      let before: { x: number; y: number } | null = null;
      try {
        const p = await win.outerPosition();
        before = { x: p.x, y: p.y };
      } catch {
        /* position illisible : on tente quand même */
      }
      let nativeOk = false;
      try {
        await win.startDragging();
        // Le drag natif bloque jusqu'au relâcher : si on arrive ici
        // sans erreur, le compositor a géré le déplacement.
        nativeOk = true;
      } catch {
        nativeOk = false;
      }
      if (nativeOk) {
        persistXY();
        return;
      }
      // 2. Fallback manuel : vérifie que la fenêtre a bougé, sinon
      // le compositor ignore aussi setPosition (fenêtre gérée/tilée) —
      // on ne persiste rien dans ce cas (pas de XY fantôme).
      const sx = e.screenX;
      const sy = e.screenY;
      const factor = (await currentMonitor().catch(() => null))?.scaleFactor || 1;
      let base: { x: number; y: number } | null = before;
      if (!base) {
        try {
          const p = await win.outerPosition();
          base = { x: p.x / factor, y: p.y / factor };
        } catch {
          return;
        }
      } else {
        base = { x: base.x / factor, y: base.y / factor };
      }
      const move = (ev: MouseEvent): void => {
        win
          .setPosition(new LogicalPosition(Math.round(base.x + (ev.screenX - sx)), Math.round(base.y + (ev.screenY - sy))))
          .catch(() => undefined);
      };
      const up = (): void => {
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
        persistXY();
      };
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    })().catch(() => undefined);

    function persistXY(): void {
      void (async () => {
        try {
          const pos = await win.outerPosition();
          const f = (await currentMonitor().catch(() => null))?.scaleFactor || 1;
          savePopupXY(pos.x / f, pos.y / f);
        } catch {
          /* position illisible : on garde l'ancien XY */
        }
      })();
    }
  }

  onMount(() => {
    let live = true;
    let doneTimer = 0;
    applyRecorderAppearance(null);
    applyPopupAppearance();
    // Scheme Caelestia + suivi fond d'écran : la popup reste ouverte peu
    // longtemps, mais un changement de wallpaper entre-temps la suit aussi.
    void fetchCaelestiaScheme().then((scheme) => {
      if (live && scheme) applyRecorderAppearance(scheme);
    });
    startCaelestiaWatch();
    let offCaelestia: (() => void) | null = null;
    void listen<CaelestiaScheme>("caelestia://scheme", (e) => {
      if (live) applyRecorderAppearance(e.payload.available ? e.payload : null);
    })
      .then((un) => {
        if (!live) {
          un();
          return;
        }
        offCaelestia = un;
      })
      .catch(() => {
        /* navigateur : pas d'événements backend */
      });
    showMochi = readPref("whisp:popup-mochi") !== "0";
    // Focus neutre à l'ouverture : sinon le WebView restaure le focus sur
    // le dernier bouton agent cliqué → Tab coincé dès la réouverture.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    void applyPopupPosition();
    // Mode courant au montage : sinon isAgent=false jusqu'au premier event,
    // le module agent manque à la première mesure et la fenêtre reste à 200.
    if (isTauri()) {
      void invoke<{ id: string; label: string }>("get_current_mode")
        .then((m) => {
          if (m?.label) mode = m.label;
          else if (m?.id) mode = m.id;
        })
        .catch(() => undefined);
    }

    // --- Events backend ---
    let cancelled = false;
    const unlistens: Array<() => void> = [];
    const sub = async (name: string, fn: (payload: unknown) => void): Promise<void> => {
      try {
        const off = await listen<unknown>(name, (e) => {
          if (!cancelled) fn(e.payload);
        });
        if (cancelled) off();
        else unlistens.push(off);
      } catch {
        /* un event indisponible ne doit jamais tuer les autres */
      }
    };
    void (async () => {
      try {
        await sub("whisp://level", (p) => {
          level.current = parseLevel(p);
        });
        await sub("whisp://state", (p) => {
          const s = parseState(p);
          const prev = statusCur;
          if (prev !== s.status) {
            if (prev === "recording" && s.status === "processing") playCue("stop");
            else if (prev === "processing" && s.status === "done") playCue("finish");
            else if (s.status === "error") playCue("error");
            statusCur = s.status;
            status = s.status;
            if (s.status === "done") {
              // Retour idle après le decay waveform (~1 s) : la boucle
              // s'arrête sur idle, pas de freeze vert.
              window.clearTimeout(doneTimer);
              doneTimer = window.setTimeout(() => {
                if (cancelled || statusCur !== "done") return;
                statusCur = "idle";
                status = "idle";
              }, 1200);
            }
          }
          message = s.message ?? null;
          if (s.mode) mode = s.mode;
          if (s.status !== "recording") partial = null;
        });
        await sub("whisp://partial", (p) => {
          const t =
            typeof p === "string"
              ? p
              : typeof (p as Record<string, unknown> | null)?.text === "string"
                ? ((p as Record<string, unknown>).text as string)
                : null;
          if (t && t.trim()) partial = t.trim();
        });
        await sub("whisp://agent-event", (p) => {
          const q = p as Record<string, unknown> | null;
          if (!q || typeof q !== "object") return;
          if (q.event !== "PermissionRequest" && q.tool_name !== "PermissionRequest") return;
          const rid = typeof q.request_id === "string" && q.request_id.trim() ? q.request_id : null;
          if (!rid) return;
          const card: Approval = {
            request_id: rid,
            session_id: String(q.session_id ?? ""),
            agent: String(q.agent ?? "agent"),
            tool: String(q.tool_name ?? "outil"),
            command: String(q.command ?? q.file_path ?? q.path ?? q.url ?? ""),
          };
          approvals =
            approvals.some((a) => a.request_id === card.request_id)
              ? approvals
              : [...approvals.slice(-2), card];
        });
        await sub("whisp://tts", (p) => {
          const q = p as Record<string, unknown> | null;
          const state = q && typeof q === "object" ? String(q.state ?? "") : "";
          if (state === "playing") {
            statusCur = "speaking";
            status = "speaking";
          } else if (state === "done" || state === "error") {
            if (statusCur === "speaking") {
              statusCur = "done";
              status = "done";
            }
          }
        });
        await sub("whisp://voice-question", (p) => {
          const q = p as Record<string, unknown> | null;
          const t = q && typeof q === "object" ? String(q.text ?? "") : "";
          if (t.trim()) voiceQuestion = t.trim();
        });
      } catch {
        /* browser dev without Tauri runtime */
      }
    })();

    playCue("start");

    // Preview navigateur hors Tauri.
    let previewId = 0;
    if (!isTauri()) {
      status = "recording";
      previewId = window.setInterval(() => {
        const t = performance.now() / 1000;
        level.current = Math.min(
          1,
          0.4 +
            0.45 * Math.abs(Math.sin(t * 2.1)) * (0.4 + 0.6 * Math.abs(Math.sin(t * 0.63 + 1.2))) +
            Math.random() * 0.06,
        );
      }, 50);
    }

    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") {
        e.preventDefault();
        cancel();
        return;
      }
      if (e.key === "Tab" && !e.shiftKey) {
        const t = e.target as HTMLElement | null;
        const tag = t?.tagName ?? "";
        const inAgent = t?.closest?.(".agent-module") != null;
        if (tag !== "INPUT" && tag !== "TEXTAREA" && tag !== "SELECT" && tag !== "BUTTON") {
          e.preventDefault();
          cycleMode();
        } else if (inAgent && tag === "BUTTON") {
          // Focus coincé dans le module agent : Tab le libère d'abord
          // (blur → body), le Tab suivant cycle normalement.
          e.preventDefault();
          (document.activeElement as HTMLElement | null)?.blur?.();
          document.body.focus?.();
        }
        return;
      }
      if (e.altKey && (e.code === "Space" || e.key === " ")) {
        e.preventDefault();
        stop();
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      live = false;
      cancelled = true;
      window.clearTimeout(doneTimer);
      window.clearInterval(previewId);
      window.removeEventListener("keydown", onKey);
      if (offCaelestia) offCaelestia();
      for (const u of unlistens) u();
    };
  });
</script>

<div class="recorder-stage">
  <div class={cardClass} role="dialog" aria-label="whisp recorder" bind:this={cardEl}>
    <div
      class="recorder-top recorder-drag"
      onmousedown={onDragStart}
      title="Glisser pour déplacer"
      role="presentation"
    >
      {#if showMochi}
        <Mochi mood={mochiMood} size={24} {level} />
      {/if}
      <button
        type="button"
        class="resize-btn"
        onclick={toggleSize}
        title={mini ? "Agrandir" : "Réduire"}
        aria-label={mini ? "Agrandir la fenêtre" : "Réduire la fenêtre"}
      >
        {@html mini ? EXPAND_ICON : SHRINK_ICON}
      </button>
      <div class="wave-wrap">
        <Waveform {level} {status} />
      </div>
      {#if mode.toLowerCase() === "voice" && voiceQuestion}
        <p class="recorder-voice-q" aria-live="polite" title={voiceQuestion}>
          {voiceQuestion}
        </p>
      {/if}
      {#if status === "error" && message && !isAgent}
        <p class="recorder-error" role="alert">
          {message}
        </p>
      {/if}
    </div>
    {#if isAgent}
      <div class="agent-module" role="status" aria-label="Agent">
        <div class="agent-header">
          <span class="agent-avatar{agentActive ? ' busy' : ''}" aria-hidden="true">
            <span class="agent-dot{agentActive ? ' on' : ''}"></span>
          </span>
          <div class="agent-title-block">
            <span class="agent-title">{agentName}{agentActive ? " · travaille…" : ""}</span>
            <span class="agent-subtitle" title={agentCwd ?? undefined}>
              {agentActive
                ? "Réponse en cours — Échap pour arrêter"
                : agentCwdBase
                  ? `Prêt — dicte ta demande · ${agentName} → ${agentCwdBase}`
                  : "Prêt — dicte ta demande, Alt+Space pour envoyer"}
            </span>
          </div>
          <span class="agent-spacer"></span>
          <button
            type="button"
            class="agent-btn"
            onclick={openAgentTerm}
            title="Ouvrir le terminal agent"
          >
            Terminal
          </button>
          {#if agentActive}
            <button
              type="button"
              class="agent-btn stop"
              onclick={stopAgent}
              disabled={agentStopping}
              title="Stopper l'agent en cours (Ctrl-C)"
            >
              {agentStopping ? "Stop…" : "Stop"}
            </button>
          {/if}
        </div>
        {#if status === "error" && message}
          <div class="agent-error" role="alert">
            <span class="agent-error-msg">{message}</span>
            <button
              type="button"
              class="agent-btn"
              onclick={openAgentTerm}
              title="Ouvrir le terminal agent"
            >
              Ouvrir le terminal
            </button>
          </div>
        {/if}
        {#each approvals.slice(0, 1) as a (a.request_id)}
          <ApprovalCard card={a} onDecide={decide} autoFocus={status !== "recording"} />
        {/each}
        <span class="agent-output-label">Dernière sortie</span>
        {#if agentOutput}
          <pre class="agent-output" aria-label="Sortie agent">{agentOutput}</pre>
        {:else}
          <p class="agent-output-empty">En attente de sortie…</p>
        {/if}
      </div>
    {/if}
    <div class="recorder-bar">
      <div class="bar-left">
        <button type="button" class="mode-btn" onclick={cycleMode} title="Changer de mode (Tab)">
          {#if status === "recording"}
            <span class="rec-dot" aria-label="Enregistrement en cours"></span>
          {:else}
            <span class="mic">{@html MIC_ICON}</span>
          {/if}
          <span class="mode-name">{mode}</span>
          <kbd>Tab</kbd>
        </button>
      </div>
      <div class="recorder-actions">
        <button type="button" class="rec-btn stop" onclick={stop}>
          Stop <kbd>Alt</kbd>
          <kbd>Espace</kbd>
        </button>
        <button type="button" class="rec-btn cancel" onclick={cancel}>Cancel <kbd>Échap</kbd></button>
      </div>
    </div>
  </div>
</div>
