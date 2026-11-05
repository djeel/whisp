# whisp — dictée vocale locale (Superwhisper-like, Hyprland/Wayland)

Alt+Espace → parle → ton texte se colle où est ton curseur.

## Lancer (service)

```bash
systemctl --user status whisp   # actif au login
~/.local/bin/whisp toggle       # = Alt+Espace
```

## Pipeline

```
voix → STT (Fish cloud si mode cloud + clé, sinon faster-whisper local medium)
     → texte nettoyé (speakers/cues stripés)
     → LLM Ollama whisp-nanbeige local (Nanbeige 4.2-3B, 100 % GPU, reformule selon le mode, reste en FR)
     → wl-copy + Ctrl+V (unicode-safe)
```

## MCP (pilotage par agent)

```json
{"command": "~/.local/bin/whisp-mcp"}
```

Source checkout : `<repo>/mcp_server.py` (voir `.mcp.json.example`).
12 tools : status/toggle/start/stop/cancel, set_mode, list_modes,
set_stt (local/cloud_fish_audio par mode), list_mics, get/set_config, history.
L'app doit tourner (socket `$XDG_RUNTIME_DIR/whisp-<uid>.sock`, fallback `/tmp`).

## Config (`~/.config/whisp/config.json`)

- `language`: `fr` (défaut) | `en` | `auto`
- `sttModel`: `medium` (défaut) | `small` | `base`
- `inputDevice`: nom cpal ou vide = défaut système
- `theme`: `system` | `dark` | `light`
- `popupPos`: `top-center` (défaut) | `center` | `bottom-center`

Clé Fish : keyring OS -> `secret-tool` (Secret Service, chiffré) -> `~/.config/whisp/fish_audio.key` (600, dernier recours).
Sans clé, le cloud retombe en local. Le crate keyring Rust est contourné via CLI quand il échoue.

Socket local : `$XDG_RUNTIME_DIR/whisp-<uid>.sock` (0600) + token `~/.config/whisp/socket.token` (600).
Le bridge MCP l'injecte seul ; `toggle` nu (Hyprland) reste exempt.

## Installer (nouvelle machine)

Rapide (bundle précompilé de la dernière release, recommandé) :

```bash
curl -fsSL https://raw.githubusercontent.com/djeel/whisp/main/install.sh | bash -s -- --bundle
```

Source (build local, ~long) :

```bash
curl -fsSL https://raw.githubusercontent.com/djeel/whisp/main/install.sh | bash
```

## Mettre à jour

Dans l'app : Settings → Mises à jour → Vérifier → Installer. Ou relancer
`install.sh` (idempotent : pull + rebuild + service).

## Limites agent/voix

- Agent : une seule requête à la fois (`agent_running`), Échap envoie Ctrl-C au tmux + coupe la voix. Prompt tronqué à 4000 chars (frontière char, jamais de multi-octet coupé).
- Voix : Échap lève `voice_cancel`, le thread TTS s'arrête avant de jouer. Fichiers `/tmp/whisp-tts-*.mp3` nettoyés.
- Flatpak : audio low-latency + global-shortcut + wl-copy via portals, tester avec `flatpak run --talk-name=org.freedesktop.portal.*`. Préfère le .deb/AppImage hors sandbox pour Hyprland.
