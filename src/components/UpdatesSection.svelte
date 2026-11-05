<script lang="ts">
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
    try {
      const update = await checkUpdate();
      if (update) {
        available = update.version;
        status = `Nouvelle version ${update.version} disponible.`;
      } else {
        status = "whisp est à jour.";
      }
    } catch {
      try {
        const r = await invoke<UpdateInfo>("check_updates");
        if (r.current) version = r.current;
        if (r.updateAvailable && r.latest) {
          available = r.latest;
          status = `Nouvelle version ${r.latest} disponible.`;
        } else {
          status = r.message ?? "whisp est à jour.";
        }
        if (r.url) void openUrl(r.url).catch(() => undefined);
      } catch (err) {
        status = `Échec : ${String(err instanceof Error ? err.message : err)}`;
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
        return;
      }
      await update.downloadAndInstall((e: DownloadEvent) => {
        if (e.event === "Started") progress = 0;
        else if (e.event === "Progress") progress = Math.min(99, (progress ?? 0) + 1);
        else if (e.event === "Finished") progress = 100;
      });
      status = "Mise à jour installée. Relancez whisp (le service systemd repartira seul).";
    } catch (e) {
      status = `Échec : ${String(e instanceof Error ? e.message : e)}`;
    } finally {
      downloading = false;
    }
  }
</script>

<section class="panel" aria-labelledby="updates-title">
  <div class="card-head">
    <h2 id="updates-title">Mises à jour</h2>
    <p class="card-hint">Version installée et dernière release GitHub.</p>
  </div>
  <div class="field-label">
    <span>Version installée</span>
    <span class="field-hint">{version ?? "…"}</span>
  </div>
  <div class="row">
    <button type="button" class="ghost" onclick={() => void check()} disabled={checking || downloading}>
      {checking ? "Vérification…" : "Vérifier"}
    </button>
    {#if status}
      <span class="field-hint" role="status">{status}</span>
    {/if}
  </div>
  {#if available}
    <div class="row">
      <button type="button" class="primary" onclick={() => void install()} disabled={downloading}>
        {downloading ? `Installation…${progress !== null ? ` ${progress}%` : ""}` : `Installer ${available}`}
      </button>
    </div>
  {/if}
</section>
