 # Changelog
 
## v0.1.2 — QoL + Flatpak

### Nouveau
- Raccourcis configurables : Agents & Voix → Raccourcis (cliquer, appuyer, sauvegardé + réenregistré à chaud, sans redémarrage).
- Bandeau onboarding Dictation quand ni Ollama ni clé Fish (guide vers Agents & Voix).
- Packaging Flatpak : manifest, métadonnées AppStream, job CI (bundle uploadé sur les releases).
- Page Chat entièrement refaite (header identité modèle, bulles, état vide hero).

### Correctifs
- Recorder déplaçable partout (drag natif vérifié + fallback manuel vérifié, XY persisté).
- Waveform : contraste garanti (parse robuste + garde luminance, fini le noir-sur-noir).
- Toggles : knob blanc qui glisse (animation `translateX` restaurée), plus de double outline sur les inputs.
- Chat input/bouton même hauteur ; carte Session agent polie ; Dictation restructurée (hero + hints).
- install.sh --bundle : vérif taille anti-tronçon, extraction AppImage sans FUSE, repli .deb.
- Updater : `latest.json` signé publié (clé minisign dédiée, `createUpdaterArtifacts`).

 ## v0.1.1 — Refonte Svelte + stabilisation

### Frontend : React → Svelte 5
- Port complet vers Svelte 5 (runes `$state`/`$derived`/`$effect`), zéro React restant.
- Bundle JS **328 → ~167 Ko** (÷2), `svelte-check` 0 erreur.
- Dropdowns customs partout : fini les `<select>` natifs qui figeaient WebKitGTK.
- 1 seul `get_config` au mount (single-flight partagé), debounce partagé avec flush.
- Page Chat supprimée (inutile au projet) ; Dictation regroupe Modes + Vocabulaire.
- Settings réorganisés : Dictation / Statistics / Application / Apparence / Agents & Voix.
- Historique des dictées + sidebar Mochi retirés (non utilisés).

### Backend : fini les freezes
- Toutes les commandes Tauri bloquantes passées en `async` + `spawn_blocking` :
  `agent_status`, `list_input_devices`, `agent_last_output`, `chat_command`.
- Tous les spawns process avec deadline (`run_timed`/`run_timed_out`) :
  tmux, hyprctl, grim, pgrep, pkill, install-hook, reload Hyprland.
- Cache `have_bin` sans spawn (`LazyLock`, scan PATH direct).
- Pipeline dictée : requête daemon réellement envoyée (`write_frame` + timeouts),
  erreur centrale émise (plus de `processing` infini), fallback texte brut si le LLM échoue.
- `MAX_MSG` défini dans le daemon STT (64 Mo).
- Watchdog thread principal (log lag >200 ms, BLOCKED >2 s).
- Config : verrou global + écriture atomique tmp+rename.

### Fenêtres & système
- Drag recorder natif compositor (`startDragging`), fini le `setPosition` ignoré par Wayland.
- Fenêtres épinglées bureau 1 (règles Hyprland), Alt+Espace libéré du menu fenêtre GNOME.
- Logo Mochi unifié : toutes les icônes Tauri régénérées depuis `logo.svg`.
- Sidebar fixe (fini le `sticky` qui tuait les hit-tests WebKit au scroll).
