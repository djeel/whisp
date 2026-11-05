<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri";

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

  let shortcuts = $state<ShortcutInfo[]>(FALLBACK_SHORTCUTS);
  let msg = $state<string | null>(null);
  let reloading = $state(false);

  onMount(() => {
    invoke<ShortcutInfo[]>("list_shortcuts")
      .then((rows) => {
        if (Array.isArray(rows) && rows.length > 0) shortcuts = rows;
      })
      .catch(() => {
        /* backend en cours : liste statique */
      });
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
</script>

<section class="panel">
  <div class="card-head">
    <h2>Raccourcis</h2>
    <p class="card-hint">Tous les raccourcis whisp, puis recharge Hyprland.</p>
  </div>
  <ul class="shortcut-list">
    {#each shortcuts as s (s.id)}
      <li class="row shortcut">
        <span class="shortcut-name" title={s.description}>{s.label}</span>
        <span class="keycap static">{s.keys}</span>
      </li>
    {/each}
  </ul>
  <div class="row">
    <button type="button" class="ghost" onclick={reload} disabled={reloading}>
      {reloading ? "Rechargement…" : "Recharger Hyprland"}
    </button>
    {#if msg}
      <span class="field-hint" role="status">{msg}</span>
    {/if}
  </div>
</section>
