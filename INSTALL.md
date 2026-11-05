# Installation

## Recommandé : Flatpak (Flathub ou release GitHub)

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
