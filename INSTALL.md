# Installation

## macOS (Apple Silicon, dmg signé par CI)

```sh
# Depuis la release GitHub :
open whisp_*_aarch64.dmg   # glisser whisp dans /Applications
```

Au premier lancement : clic droit → Ouvrir (binaire non notarié,
signature ad-hoc). Requis :
- **Micro** : autoriser dans Réglages → Confidentialité → Microphone.
- **Collage auto** : `pbcopy`/`pbpaste` natifs + frappe `Cmd+V` via
  osascript (autoriser l'automatisation si demandé) ; sans permission
  d'accessibilité le texte reste dans le presse-papiers.
- **Raffinage LLM** : Ollama local (`ollama pull whisp-nanbeige`) ou
  cloud (clé Anthropic/OpenAI, stockée dans le Keychain macOS).
- **STT** : faster-whisper local (`pip install faster-whisper`,
  venv `~/Library/Application Support/whisp/fw`) ou Fish cloud.
- **Démarrage auto** : Réglages → Lancer au démarrage (LaunchAgent
  `~/Library/LaunchAgents/com.whisp.app.plist`).

## iOS (cloud-only, via Xcode sur le Mac du propriétaire)

Pas de faster-whisper ni d'Ollama locaux (sandbox iOS : aucun
sous-processus) — la dictée exige les deux clés cloud :
- STT : clé Fish Audio (console fish.audio).
- Raffinage : clé Anthropic ou OpenAI.

```sh
bun install --frozen-lockfile
bunx tauri ios init        # une fois : génère le projet Xcode
bunx tauri ios build       # depuis le Mac signé (profil owner)
```

Mode agent non applicable (pas de tmux/terminal), historique et
raffinage identiques au desktop.

## Linux — recommandé : Flatpak (Flathub ou release GitHub)

```sh
# Depuis la release GitHub (v0.1.2+) :
flatpak install --user whisp.flatpak
flatpak run com.whisp.app
```

Le Flatpak embarque tout (runtime Freedesktop 24.08, binaire whisp).
Permissions : Wayland, audio (PipeWire), presse-papiers, config persistée
(`~/.config/whisp`, `~/.local/share/whisp`). Micro via le portal PipeWire :
autorisez l'accès au premier lancement.

## Alternative : .deb (Debian/Ubuntu)

```sh
sudo dpkg -i whisp_0.1.2_amd64.deb
```

## Alternative : AppImage (sans installation)

```sh
chmod +x whisp_0.1.2_amd64.AppImage
./whisp_0.1.2_amd64.AppImage
```

Sans FUSE : `./whisp.AppImage --appimage-extract` puis lancer
`squashfs-root/AppRun`.

## Depuis les sources (développeurs)

```sh
./install.sh            # build complet (Hyprland)
./install.sh --bundle   # binaire précompilé uniquement (vérif taille + extraction sans sudo)
```

## Premier lancement

Sans Ollama ni clé Fish, la dictée colle le texte brut. Pour le
raffinage + la ponctuation :

- **Offline (recommandé)** : `ollama pull whisp-nanbeige`
- **Cloud** : clé Fish Audio dans whisp → Agents & Voix → Voix Fish Audio
  (console fish.audio → TTS → voix, copier le `reference_id` + la clé API).

Un bandeau dans l'onglet Dictation vous guide tant que rien n'est configuré.

## Raccourci global

- **Hyprland** : `ALT + Space` (bind installé par `install.sh`).
- **GNOME** : custom keybinding `~/.local/bin/whisp-toggle` sur `Alt+Espace`
  (le plugin Tauri ne traverse pas Wayland ; le bind natif du compositor si).
- Le raccourci est **configurable** : Agents & Voix → Raccourcis →
  cliquez sur un raccourci, appuyez la nouvelle combinaison.
