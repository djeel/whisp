<script lang="ts">
  import Button from "./Button.svelte";
  import { onMount } from "svelte";
  import { check as checkUpdate, type DownloadEvent } from "@tauri-apps/plugin-updater";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { invoke } from "../lib/tauri";

  interface UpdateInfo {
    current: string;
    latest?: string | null;
    updateAvailable: boolean;
    url?: string | null;
    message?: string | null;
  }

  let version = $state<string | null>(null);
  let status = $state<string | null>(null);
  let available = $state<string | null>(null);
  let checking = $state(false);
  let downloading = $state(false);
  let progress = $state<number | null>(null);
  let kind = $state<"idle" | "ok" | "update" | "error">("idle");

  onMount(() => {
    invoke<string>("app_version")
      .then((v) => {
        version = v;
      })
      .catch(() => {
        version = null;
      });
  });

  async function check(): Promise<void> {
    checking = true;
    status = null;
    available = null;
    kind = "idle";
    try {
      const update = await checkUpdate();
      if (update) {
        available = update.version;
        status = `Nouvelle version ${update.version} disponible.`;
        kind = "update";
      } else {
        status = "whisp est à jour.";
        kind = "ok";
      }
    } catch {
      try {
        const r = await invoke<UpdateInfo>("check_updates");
        if (r.current) version = r.current;
        if (r.updateAvailable && r.latest) {
          available = r.latest;
          status = `Nouvelle version ${r.latest} disponible.`;
          kind = "update";
        } else {
          status = r.message ?? "whisp est à jour.";
          kind = r.message?.includes("Échec") || r.message?.includes("Aucune") ? "error" : "ok";
        }
        if (r.url) void openUrl(r.url).catch(() => undefined);
      } catch (err) {
        status = `Échec : ${String(err instanceof Error ? err.message : err)}`;
        kind = "error";
      }
    } finally {
      checking = false;
    }
  }

  async function install(): Promise<void> {
    downloading = true;
    progress = 0;
    status = null;
    try {
      const update = await checkUpdate();
      if (!update) {
        status = "whisp est à jour.";
        kind = "ok";
        return;
      }
      await update.downloadAndInstall((e: DownloadEvent) => {
        if (e.event === "Started") progress = 0;
        else if (e.event === "Progress") progress = Math.min(99, (progress ?? 0) + 1);
        else if (e.event === "Finished") progress = 100;
      });
      status = "Mise à jour installée. Relancez whisp (le service systemd repartira seul).";
      kind = "ok";
    } catch (err) {
      status = `Échec : ${String(err instanceof Error ? err.message : err)}`;
      kind = "error";
    } finally {
      downloading = false;
    }
  }
</script>

<section class="panel updates" aria-labelledby="updates-title">
  <div class="card-head">
    <h2 id="updates-title">Mises à jour</h2>
    <p class="card-hint">Version installée et dernière release GitHub.</p>
  </div>

  <div class="updates-version">
    <span class="updates-label">Version installée</span>
    <span class="updates-pill">{version ?? "…"}</span>
  </div>

  {#if status}
    <p
      class="updates-status"
      class:ok={kind === "ok"}
      class:update={kind === "update"}
      class:error={kind === "error"}
      role="status"
    >
      <span class="updates-dot" aria-hidden="true"></span>
      {status}
    </p>
  {/if}

  {#if downloading && progress !== null}
    <div class="updates-progress" role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100} aria-label="Téléchargement de la mise à jour">
      <div class="updates-bar" style:width={`${progress}%`}></div>
    </div>
  {/if}

  <div class="row updates-actions">
    <Button onclick={() => void check()} disabled={checking || downloading} busy={checking}>
      {checking ? "Vérification…" : "Vérifier"}
    </Button>
    {#if available}
      <Button variant="primary" onclick={() => void install()} disabled={downloading} busy={downloading}>
        {downloading ? `Installation…${progress !== null ? ` ${progress}%` : ""}` : `Installer ${available}`}
      </Button>
    {/if}
  </div>
</section>

<style>
  .updates-version {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
  }
  .updates-label {
    flex: 1;
    min-width: 0;
  }
  .updates-pill {
    font-family: ui-monospace, monospace;
    font-size: 13px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--bg-input);
    border: 1px solid var(--line);
    white-space: nowrap;
  }
  .updates-status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--muted);
  }
  .updates-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    flex-shrink: 0;
  }
  .updates-status.ok {
    color: var(--text);
  }
  .updates-status.ok .updates-dot {
    background: var(--green);
  }
  .updates-status.update {
    color: var(--text);
  }
  .updates-status.update .updates-dot {
    background: var(--accent);
    animation: update-pulse 1.6s infinite ease-in-out;
  }
  .updates-status.error {
    color: var(--red);
  }
  .updates-status.error .updates-dot {
    background: var(--red);
  }
  @keyframes update-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  .updates-progress {
    height: 8px;
    border-radius: 999px;
    background: var(--bg-input);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .updates-bar {
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .updates-actions {
    margin-top: 4px;
  }
</style>
