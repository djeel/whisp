<script module lang="ts">
  /** Option du Dropdown custom (valeur opaque, label affiché). */
  export interface DropdownOption {
    value: string;
    label: string;
    /** Infobulle ; défaut = label. */
    title?: string;
  }
</script>

<script lang="ts">
  /** Dropdown custom : remplace le <select> natif qui fige WebKitGTK.
   * Bouton + listbox, clavier ↑↓/Enter/Échap, click-outside. Testable headless
   * (rôles button/listbox/option, aucun appel natif). */
  let {
    options,
    value = $bindable(""),
    label = "",
    title = "",
    placeholder = "",
    onselect,
  }: {
    options: DropdownOption[];
    value?: string;
    label?: string;
    title?: string;
    placeholder?: string;
    onselect?: (value: string) => void;
  } = $props();

  let open = $state(false);
  let highlight = $state(0);
  let root = $state<HTMLDivElement | null>(null);
  let btn = $state<HTMLButtonElement | null>(null);
  let list = $state<HTMLUListElement | null>(null);

  const selected = $derived(options.find((o) => o.value === value) ?? null);
  const shown = $derived(selected?.label ?? (value ? value : (placeholder || "—")));

  function openList(): void {
    // Liste vide (backend en cours) : ne pas ouvrir de listbox vide.
    if (options.length === 0) return;
    const idx = options.findIndex((o) => o.value === value);
    highlight = idx >= 0 ? idx : 0;
    open = true;
  }

  function closeList(refocus: boolean): void {
    open = false;
    if (refocus) btn?.focus();
  }

  function choose(v: string): void {
    value = v;
    open = false;
    btn?.focus();
    onselect?.(v);
  }

  function onBtnKey(e: KeyboardEvent): void {
    if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (!open) openList();
    }
  }

  function scrollActive(): void {
    list?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  }

  function onListKey(e: KeyboardEvent): void {
    if (options.length === 0) return;
    if (e.key === "Escape") {
      e.preventDefault();
      closeList(true);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      highlight = (highlight + 1) % options.length;
      scrollActive();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      highlight = (highlight - 1 + options.length) % options.length;
      scrollActive();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = options[highlight];
      if (opt) choose(opt.value);
    } else if (e.key === "Tab") {
      closeList(false);
    }
  }

  $effect(() => {
    if (!open) return;
    list?.focus();
    const onDown = (e: PointerEvent): void => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  });
</script>

<div class="dropdown" bind:this={root}>
  <button
    type="button"
    bind:this={btn}
    class="dropdown-btn"
    aria-haspopup="listbox"
    aria-expanded={open}
    {title}
    aria-label={label || title || selected?.label || placeholder}
    onclick={() => (open ? closeList(false) : openList())}
    onkeydown={onBtnKey}
  >
    <span>{shown}</span>
  </button>
  {#if open}
    <ul
      bind:this={list}
      role="listbox"
      tabindex="-1"
      aria-label={label || title}
      class="dropdown-list"
      onkeydown={onListKey}
    >
      {#each options as o, i (o.value)}
        <li
          role="option"
          tabindex="-1"
          aria-selected={o.value === value}
          data-active={i === highlight ? "true" : "false"}
          title={o.title ?? o.label}
          class:active={i === highlight}
          class:selected={o.value === value}
          onclick={() => choose(o.value)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              choose(o.value);
            }
          }}
          onpointerenter={() => {
            highlight = i;
          }}
        >
          {o.label}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
    min-width: 0;
  }
  .dropdown-btn {
    width: 100%;
    max-width: 100%;
    display: flex;
    align-items: center;
    background: var(--bg-input);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 7px 28px 7px 12px;
    color: var(--text);
    font: inherit;
    cursor: pointer;
    background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6' fill='none'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%23888' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 10px center;
  }
  .dropdown-btn:hover {
    border-color: var(--muted);
  }
  .dropdown-btn > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dropdown-list {
    position: absolute;
    z-index: 50;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: 10px;
    max-height: 240px;
    overflow-y: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  }
  .dropdown-list [role="option"] {
    padding: 7px 10px;
    border-radius: 7px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dropdown-list [role="option"].active {
    background: var(--hover-wash);
  }
  .dropdown-list [role="option"].selected {
    font-weight: 600;
  }
</style>
