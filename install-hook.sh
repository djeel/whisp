#!/usr/bin/env bash
# whisp-hook installer — branche whisp sur tes agents (Claude Code, etc).
# Usage: bash ~/Projets/whisp/install-hook.sh [--agent <nom>]
set -euo pipefail

HOOK_BIN="$HOME/.local/share/whisp/bin/whisp-hook"
[ -x "$HOOK_BIN" ] || { echo "whisp-hook introuvable ($HOOK_BIN). Build d'abord whisp."; exit 1; }

# --- Claude Code (~/.claude/settings.json) ---
mkdir -p ~/.claude
SETTINGS=~/.claude/settings.json
[ -f "$SETTINGS" ] || echo '{}' > "$SETTINGS"
python3 - "$SETTINGS" "$HOOK_BIN" <<'EOF'
import json, sys
path, hook = sys.argv[1], sys.argv[2]
d = json.load(open(path))
hooks = d.setdefault("hooks", {})
for ev in ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
           "PostToolUseFailure", "Notification", "Stop", "SubagentStart", "SubagentStop", "SessionEnd"]:
    lst = hooks.setdefault(ev, [])
    entry = {"type": "command", "command": f"'{hook}' {ev}"}
    if entry not in lst:
        lst.append(entry)
json.dump(d, open(path, "w"), indent=2)
print(f"Claude Code branché ({path})")
EOF

echo "OK. Tes sessions Claude Code apparaîtront dans whisp (pills + island)."
echo "Test : echo '{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"t1\",\"prompt\":\"hello\"}' | '$HOOK_BIN' --agent demo"
