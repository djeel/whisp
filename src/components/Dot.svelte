<!-- Pastille statut + badge — remplace `.dot/.dots/.rec-dot/.agent-dot/.agent-badge`
  et leurs 6 copies (updates-dot, setup-dot…). Couleurs = tokens status.
  `tone` : working | blocked | done | idle. Rond 8px ou pill badge. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    tone = "idle" as "working" | "blocked" | "done" | "idle",
    badge = false,
    pulse = false,
    children = undefined as Snippet | undefined,
  }: {
    tone?: "working" | "blocked" | "done" | "idle";
    badge?: boolean;
    pulse?: boolean;
    children?: Snippet | undefined;
  } = $props();

  const color =
    tone === "working"
      ? "var(--green)"
      : tone === "blocked"
        ? "var(--red)"
        : tone === "done"
          ? "var(--accent)"
          : "var(--muted)";
</script>

{#if badge}
  <span class="badge is-{tone}" style:border-color={color} style:color={tone === "idle" ? "var(--muted)" : color}>
    {#if children}{@render children()}{:else}{tone}{/if}
  </span>
{:else}
  <i class="dot" class:pulse style:background={color} aria-hidden="true"></i>
{/if}

<style>
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
    flex: 0 0 auto;
  }
  .dot.pulse {
    animation: dot-pulse 1.4s infinite ease-in-out;
  }
  @keyframes dot-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }
  .badge {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 12px;
    border-radius: 999px;
    border: 1px solid;
    background: transparent;
    white-space: nowrap;
  }
  .badge.is-working {
    background: color-mix(in srgb, var(--green) 12%, transparent);
  }
</style>
