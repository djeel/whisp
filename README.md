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

Clé Fish : `~/.config/whisp/fish_audio.key` (600). Sans clé, le cloud
retombe en local. Le keyring système est contourné (crate Rust défectueuse
sur ce host).

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
