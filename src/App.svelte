<script lang="ts">
  import { onMount } from "svelte";
  import Recorder from "./routes/Recorder.svelte";
  import Settings from "./routes/Settings.svelte";
  import WaveBench from "./routes/WaveBench.svelte";
  import { applyTheme, readPref } from "./lib/prefs";

  const win = new URLSearchParams(window.location.search).get("window");

  onMount(() => {
    if (win === "recorder") {
      document.documentElement.classList.add("is-recorder");
      document.body.classList.add("is-recorder");
    } else if (win !== "recorder") {
      applyTheme(readPref("whisp:theme"));
    }
  });
</script>

{#if win === "recorder"}
  <Recorder />
{:else if win === "wavebench"}
  <WaveBench />
{:else}
  <Settings />
{/if}
