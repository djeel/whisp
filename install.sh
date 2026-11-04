#!/usr/bin/env bash
# whisp — one-shot installer (CachyOS/Arch + Hyprland/Wayland).
# Usage: curl -fsSL https://raw.githubusercontent.com/<you>/whisp/main/install.sh | bash
set -euo pipefail

REPO="${WHISP_REPO:-https://github.com/<you>/whisp.git}"
PREFIX="${WHISP_PREFIX:-$HOME/.local}"
SRC_DIR="${WHISP_SRC:-$HOME/.local/share/whisp-app}"
FW_DIR="$HOME/.local/share/whisp/fw"
BIN="$PREFIX/bin/whisp"
SERVICE_NAME="whisp"

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
