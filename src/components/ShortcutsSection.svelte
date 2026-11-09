<script lang="ts">
  import Button from "./Button.svelte";
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri";

  interface HotkeyRow {
    id: string;
    label: string;
    keys: string;
    raw: string;
    editable: boolean;
    description: string;
  }

  const FALLBACK: HotkeyRow[] = [
    { id: "toggle", label: "Dicter / arrêter", keys: "Alt + Espace", raw: "alt+Space", editable: true, description: "Bascule l'enregistrement (global)" },
    { id: "push-to-talk", label: "Push-to-talk", keys: "Ctrl + Alt + Espace (maintenir)", raw: "control+alt+Space", editable: true, description: "Enregistre tant que les touches sont maintenues" },
    { id: "cycle-mode", label: "Changer de mode", keys: "Ctrl + Alt + M", raw: "control+alt+M", editable: true, description: "Cycle les modes de dictée" },
    { id: "cancel", label: "Annuler", keys: "Échap", raw: "Escape", editable: false, description: "Annule la dictée en cours (fenêtre popup)" },
  ];

  let rows = $state<HotkeyRow[]>(FALLBACK);
  let msg = $state<string | null>(null);
  let reloading = $state(false);
  let capturing: string | null = $state(null);
  let captureKeys = $state("");
  let saving = $state<string | null>(null);
  let resetting = $state(false);

  function refresh(): void {
    invoke<HotkeyRow[]>("list_hotkeys")
      .then((r) => {
        if (Array.isArray(r) && r.length > 0) rows = r;
      })
      .catch(() => {
        /* backend en cours : liste statique */
      });
  }

  onMount(() => {
    refresh();
  });

  function reload(): void {
    reloading = true;
    msg = null;
    invoke<string>("reload_hyprland")
      .then((m) => {
        msg = m;
      })
      .catch((e) => {
        msg = `Échec : ${String(e)}`;
      })
      .finally(() => {
        reloading = false;
      });
  }

  /** Traduit un KeyboardEvent en `control+alt+Space` (format backend). */
  function eventToRaw(e: KeyboardEvent): string | null {
    // Ignoré : modificateurs seuls, Tab (navigation), Échap (annule).
    if (["Control", "Alt", "Shift", "Meta", "Tab", "Escape"].includes(e.key)) return null;
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("control");
    if (e.altKey) parts.push("alt");
    if (e.shiftKey) parts.push("shift");
    if (e.metaKey) parts.push("super");
    // Code physique (KeyM, Space, F1…) : indépendant du layout (AZERTY ok).
    let code = e.code;
    if (code.startsWith("Key")) code = code.slice(3);
    else if (code.startsWith("Digit")) code = code.slice(5);
    parts.push(code);
    return parts.join("+");
  }

  function startCapture(id: string): void {
    capturing = id;
    captureKeys = "Appuyez sur les touches… (Échap pour annuler)";
    msg = null;
    // Suspend les globaux : sinon l'ancien binding se déclenche pendant
    // la frappe (Alt+Espace ouvrait le recorder au lieu d'être capturé).
    void invoke("pause_hotkeys").catch(() => undefined);
  }

  function cancelCapture(): void {
    capturing = null;
    captureKeys = "";
    void invoke("resume_hotkeys")
      .then(() => refresh())
      .catch(() => undefined);
  }

  function resetDefaults(): void {
    resetting = true;
    msg = null;
    invoke<string>("reset_hotkeys")
      .then((m) => {
        msg = m;
        refresh();
      })
      .catch((e: unknown) => {
        msg = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        resetting = false;
      });
  }

  function onCaptureKey(e: KeyboardEvent): void {
    if (capturing === null) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      cancelCapture();
      return;
    }
    const raw = eventToRaw(e);
    if (!raw) return; // modificateur seul : on attend la touche
    saveHotkey(capturing, raw);
  }

  function saveHotkey(id: string, raw: string): void {
    saving = id;
    msg = null;
    invoke<string>("set_hotkey", { id, keys: raw })
      .then((m) => {
        msg = m;
        refresh();
      })
      .catch((e: unknown) => {
        msg = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        saving = null;
        capturing = null;
        captureKeys = "";
      });
  }
</script>

<svelte:window onkeydown={onCaptureKey} />

<section class="panel">
  <div class="card-head">
    <h2>Raccourcis</h2>
    <p class="card-hint">Cliquez sur un raccourci pour le changer. Échap annule la capture.</p>
  </div>
  <ul class="shortcut-list">
    {#each rows as s (s.id)}
      <li class="row shortcut">
        <span class="shortcut-name" title={s.description}>{s.label}</span>
        {#if s.editable}
          <button
            type="button"
            class="keycap keycap-btn"
            class:capturing={capturing === s.id}
            onclick={() => (capturing === s.id ? cancelCapture() : startCapture(s.id))}
            disabled={saving !== null}
            title={s.description + " — cliquer pour changer"}
            aria-label={`Changer le raccourci ${s.label} (actuel : ${s.keys})`}
          >
            {#if capturing === s.id}
              {saving === s.id ? "Enregistrement…" : captureKeys}
            {:else}
              {s.keys}
            {/if}
          </button>
        {:else}
          <span class="keycap static" title={s.description}>{s.keys}</span>
        {/if}
      </li>
    {/each}
  </ul>
   <div class="row">
    <Button onclick={reload} disabled={reloading} busy={reloading}>
      {reloading ? "Rechargement…" : "Recharger Hyprland"}
    </Button>
    <Button onclick={resetDefaults} disabled={resetting || saving !== null} busy={resetting} title="Restaure Alt+Espace, Ctrl+Alt+Espace, Ctrl+Alt+M">
      {resetting ? "Réinitialisation…" : "Réinitialiser"}
    </Button>
     {#if msg}
       <span class="field-hint" role="status">{msg}</span>
     {/if}
</section>

<style>
  .keycap-btn {
    cursor: pointer;
    font: inherit;
    min-width: 140px;
    text-align: center;
  }
  .keycap-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text);
  }
  .keycap-btn.capturing {
    border-color: var(--accent);
    color: var(--accent);
    animation: capture-pulse 1s infinite ease-in-out;
  }
  @keyframes capture-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.55; }
  }
</style>
