<script lang="ts">
  /** Interrupteur custom (même langage que App.css : 40x22px, 150ms, AA). */
  let {
    checked = $bindable(false),
    label = "",
    onchange,
  }: {
    checked?: boolean;
    label?: string;
    onchange?: (v: boolean) => void;
  } = $props();

  function toggle(): void {
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class="switch"
  class:on={checked}
  onclick={toggle}
>
  <span class="switch-knob" aria-hidden="true"></span>
</button>
