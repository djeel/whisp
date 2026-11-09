<!-- Bouton unique whisp — remplace `.primary/.ghost/.rec-btn/.agent-btn/.keycap-btn`.
  Façon Herdr : un composant, variantes typées, zéro langage bouton dispersé.
  `variant` : primary (accent) | ghost (bordure) | danger-ghost (stop/deny).
  `size` : sm (agent-btn) | md (défaut). État chargement via `busy`. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "ghost" as "primary" | "ghost" | "danger-ghost",
    size = "md" as "sm" | "md",
    disabled = false,
    busy = false,
    title = undefined as string | undefined,
    onclick,
    children,
  }: {
    variant?: "primary" | "ghost" | "danger-ghost";
    size?: "sm" | "md";
    disabled?: boolean;
    busy?: boolean;
    title?: string | undefined;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button
  type="button"
  class="btn is-{variant} is-{size}"
  {disabled}
  aria-busy={busy}
  {title}
  {onclick}
>
  {#if busy}<span class="btn-spinner" aria-hidden="true"></span>{/if}
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    border-radius: 10px;
    font-weight: 600;
    cursor: pointer;
    transition: filter 0.15s ease, color 0.15s ease, border-color 0.15s ease, background-color 0.15s ease;
  }
  .btn.is-md {
    padding: 8px 16px;
    font-size: 14px;
    line-height: 1.4;
  }
  .btn.is-sm {
    padding: 6px 12px;
    font-size: 13px;
    line-height: 1.3;
  }
  .btn.is-primary {
    background: var(--accent);
    color: var(--accent-text);
    border: 1px solid transparent;
  }
  .btn.is-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }
  .btn.is-ghost {
    background: transparent;
    border: 1px solid var(--line);
    color: var(--muted);
    font-weight: 400;
  }
  .btn.is-ghost:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--muted);
    background: var(--hover-wash);
  }
  .btn.is-danger-ghost {
    background: transparent;
    border: 1px solid var(--red);
    color: var(--red);
    font-weight: 400;
  }
  .btn.is-danger-ghost:hover:not(:disabled) {
    background: var(--hover-wash);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
