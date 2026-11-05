#!/usr/bin/env bash
# whisp — one-shot installer (CachyOS/Arch + Hyprland/Wayland).
# Usage: curl -fsSL https://raw.githubusercontent.com/djeel/whisp/main/install.sh | bash [--bundle]
#   --bundle : installe le .deb précompilé de la dernière release (rapide).
#   défaut   : clone + build source (long, développeurs).
set -euo pipefail

REPO="${WHISP_REPO:-https://github.com/djeel/whisp.git}"
PREFIX="${WHISP_PREFIX:-$HOME/.local}"
SRC_DIR="${WHISP_SRC:-$HOME/.local/share/whisp-app}"
FW_DIR="$HOME/.local/share/whisp/fw"
BIN="$PREFIX/bin/whisp"
SERVICE_NAME="whisp"

  if [ "${1:-}" = "--bundle" ]; then
  log() { printf '\033[1;34m[whisp]\033[0m %s\n' "$*"; }
  log "bundle précompilé (dernière release)..."
  command -v curl >/dev/null || { echo "curl requis"; exit 1; }
  TAG="$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/djeel/whisp/releases/latest | sed 's#.*/##')"
  [ -n "$TAG" ] || { echo "aucune release (utilise le mode source)"; exit 1; }
  DEB_URL="https://github.com/djeel/whisp/releases/download/$TAG/whisp_${TAG#v}-1_amd64.deb"
  TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
  APPIMG_URL="https://github.com/djeel/whisp/releases/download/$TAG/whisp_${TAG#v}_amd64.AppImage"
  # Taille attendue via HEAD (détecte un téléchargement tronqué : 63 Mo au
  # lieu de 84 Mo vu en prod — squashfs illisible, "Failed to open").
  EXPECTED="$(curl -fsSLI -o /dev/null -w '%{size_download}' "$APPIMG_URL" 2>/dev/null || echo 0)"
  curl -fsSL --retry 3 -o "$TMP/whisp.AppImage" "$APPIMG_URL" || die "téléchargement AppImage impossible"
  GOT="$(stat -c%s "$TMP/whisp.AppImage" 2>/dev/null || echo 0)"
  if [ "$EXPECTED" -gt 0 ] && [ "$GOT" -lt "$EXPECTED" ]; then
  die "AppImage tronquée ($GOT < $EXPECTED octets) — relance (réseau instable)"
  fi
  chmod +x "$TMP/whisp.AppImage"
   # FUSE absent ? --appimage-extract marche sans FUSE (vu en prod GNOME sans libfuse2).
  # Extraction DANS $TMP (jamais dans le cwd, nettoyé par le trap).
  (cd "$TMP" && "$TMP/whisp.AppImage" --appimage-extract >/dev/null 2>&1)
  if [ -x "$TMP/squashfs-root/AppRun" ]; then
  WBIN="$TMP/squashfs-root/usr/bin/whisp"
  [ -x "$WBIN" ] || WBIN="$TMP/squashfs-root/AppRun"
  install -Dm755 "$WBIN" "$BIN"
  log "binaire $TAG installé (extrait de l'AppImage, sans FUSE). Deps + service : relance sans --bundle, ou service manuel."
  elif command -v dpkg >/dev/null; then
  # Repli .deb : extraction sans sudo (dpkg -x, pas dpkg -i).
  curl -fsSL --retry 3 -o "$TMP/whisp.deb" "$DEB_URL" || die "téléchargement .deb impossible"
  dpkg -x "$TMP/whisp.deb" "$TMP/pkg" 2>/dev/null || die "extraction .deb impossible"
  WBIN="$(find "$TMP/pkg" -name whisp -type f -path '*bin*' | head -1)"
  [ -n "$WBIN" ] || die "binaire introuvable dans le bundle"
  install -Dm755 "$WBIN" "$BIN"
  log "binaire $TAG installé (extrait du .deb). Deps + service : relance sans --bundle, ou service manuel."
  else
  die "ni extraction AppImage (FUSE/extraction ?) ni dpkg — utilise le mode source"
  fi
  exit 0
 fi

log() { printf '\033[1;34m[whisp]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[whisp]\033[0m %s\n' "$*" >&2; exit 1; }

log "1/7 deps système (sudo requis, une seule popup)..."
if command -v paru >/dev/null; then AUR=paru; else AUR="pacman -S --needed --noconfirm"; fi
pkexec bash -c "pacman -S --needed --noconfirm ollama libayatana-appindicator wtype ccache alsa-lib pipewire-jack espeak-ng uv bun nodejs 2>&1 | tail -2 && systemctl enable ollama --now" \
  || die "installe les deps à la main : ollama libayatana-appindicator wtype uv bun nodejs"

log "2/7 sources..."
if [ -d "$SRC_DIR/.git" ]; then git -C "$SRC_DIR" pull --ff-only; else git clone --depth 1 "$REPO" "$SRC_DIR"; fi

log "3/7 venv faster-whisper (CPU int8, fiable)..."
uv venv "$FW_DIR" --python 3.14 >/dev/null 2>&1 || true
uv pip install --python "$FW_DIR/bin/python" faster-whisper 2>&1 | tail -1 || die "venv faster-whisper"

log "4/7 modèle STT small (~500 Mo, rapide)..."
"$FW_DIR/bin/python" -c "from faster_whisper import WhisperModel; WhisperModel('small', device='cpu', compute_type='int8'); print('STT-OK')"

log "5/7 LLM local qwen2.5:3b..."
ollama pull qwen2.5:3b 2>&1 | tail -1 || log "(ollama pull à relancer : ollama pull qwen2.5:3b)"

log "6/7 build (peut durer quelques minutes)..."
export WHISP_FW_PYTHON="$FW_DIR/bin/python"
(cd "$SRC_DIR" && bun install && bun run tauri build)
install -Dm755 "$SRC_DIR/src-tauri/target/release/whisp" "$BIN"

log "7/7 raccourci Alt+Space (Hyprland)..."
mkdir -p ~/.config/hypr/hyprland
if ! grep -q "whisp toggle" ~/.config/hypr/hyprland/keybinds.lua 2>/dev/null; then
  cat >> ~/.config/hypr/hyprland/keybinds.lua <<EOF

-- Voice dictation (whisp)
create_bind("ALT + Space", hl.dsp.exec_cmd("$BIN toggle"))
EOF
fi
systemctl --user enable --now ydotool 2>/dev/null || true

log "8/8 service autostart..."
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/whisp.service <<EOF
[Unit]
Description=whisp voice dictation (Superwhisper-like)
After=graphical-session.target

[Service]
ExecStart=$BIN
Restart=on-failure
RestartSec=2
Environment=WHISP_FW_PYTHON=$FW_DIR/bin/python

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now whisp
hyprctl reload 2>/dev/null || true

log "OK. $BIN tourne en fond. Alt+Space dans n'importe quelle app pour dicter."
log "Clé Fish Audio optionnelle : dans whisp > Configuration."
