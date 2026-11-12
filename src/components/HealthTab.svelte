<script lang="ts">
  import Button from "./Button.svelte";
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri";

  let report = $state<Record<string, unknown> | null>(null);
  let loading = $state(false);
  let notice = $state<string | null>(null);

  function buildLines(r: Record<string, unknown>): string[] {
    const stt = (r.stt ?? {}) as Record<string, unknown>;
    const ollama = (r.ollama ?? {}) as Record<string, unknown>;
    const claude = (r.claude ?? {}) as Record<string, unknown>;
    const last = r.last_run as Record<string, unknown> | null;
    const hist = (r.history ?? {}) as Record<string, unknown>;
    const yesNo = (v: unknown): string => (v ? "oui" : "non");
    const ollamaTxt = ollama.reachable
      ? `${String(ollama.url)} (${String(ollama.models)} modèle(s))`
      : `injoignable (${String(ollama.url ?? "")})`;
    const lastTxt = last ? `${String(last.ms)} ms · mode ${String(last.mode)}` : "aucune";
    const sidecar = stt.python3 || stt.venv_python ? "dispo" : "manquant";
    return [
      `STT daemon : ${String(stt.daemon_model ?? "arrêté")}`,
      `Python sidecar : ${sidecar}`,
      `Fish configuré : ${yesNo(stt.fish_configured)}`,
      `Ollama : ${ollamaTxt}`,
      `Claude connecté : ${yesNo(claude.connected)}`,
      `Dernière dictée : ${lastTxt}`,
      `Dictées en historique : ${String(hist.entries ?? 0)}`,
    ];
  }

  const lines = $derived(report ? buildLines(report) : []);

  async function refresh(): Promise<void> {
    loading = true;
    notice = null;
    try {
      report = await invoke<Record<string, unknown>>("health_report");
    } catch {
      notice = "Santé indisponible (backend en cours).";
      report = null;
    } finally {
      loading = false;
    }
  }

  function exportJson(): void {
    if (!report) return;
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `whisp-sante-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function copyJson(): Promise<void> {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
      notice = "Rapport copié dans le presse-papiers.";
    } catch {
      notice = "Copie impossible.";
    }
  }

  onMount(refresh);
</script>

<section class="panel">
  <div class="card-head">
    <h2>Santé</h2>
    <p class="card-hint">Daemon STT, Ollama, Claude, dernière dictée. Zéro secret : clés réduites à présent/absent.</p>
  </div>
  <div class="row">
    <Button onclick={refresh} disabled={loading} busy={loading}>Actualiser</Button>
    <Button onclick={exportJson} disabled={!report}>Export JSON</Button>
    <Button onclick={copyJson} disabled={!report}>Copier</Button>
  </div>
  {#if notice}
    <p class="field-hint" role="status">{notice}</p>
  {/if}
  {#if report}
    <ul class="health">
      {#each lines as line (line)}
        <li>{line}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .health {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }
</style>
