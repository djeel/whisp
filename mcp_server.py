#!/usr/bin/env python3
"""whisp MCP server (stdio) — control plane for the whisp dictation app.

Speaks MCP over stdio (JSON-RPC, no external deps) and forwards to the
running whisp instance over its unix control socket ($XDG_RUNTIME_DIR or /tmp fallback).

Tools:
  whisp_status      recording state + active mode
  whisp_toggle      start/stop dictation (like Alt+Space)
  whisp_start       start recording
  whisp_stop        stop + transcribe -> paste (returns transcript + refined text)
  whisp_cancel      discard current recording
  whisp_set_mode    switch dictation mode by id
  whisp_list_modes  builtin + custom modes with their STT engine
  whisp_list_mics   capture devices visible to the app
  whisp_get_config  app config (stt model, language, mic, theme...)
  whisp_history     recent dictations (transcript + refined)

Install (Claude Code / omp): copy `.mcp.json.example` to `.mcp.json` and adjust the path,
  or add manually: {"command": "~/.local/bin/whisp-mcp"}
  (source checkout: <repo>/mcp_server.py).
Requires the whisp app running (`bun run tauri dev` or the release binary).
"""
import json
import os
import socket
import sys
import tempfile
APP_DIR = os.path.dirname(os.path.abspath(__file__))

def _sock_path() -> str:
    # $XDG_RUNTIME_DIR d'abord (0600 implicite), fallback /tmp prédictible.
    run = os.environ.get("XDG_RUNTIME_DIR", "")
    name = f"whisp-{os.getuid() if hasattr(os, 'getuid') else 1000}.sock"
    if run and os.path.isdir(run):
        return os.path.join(run, name)
    return os.path.join(tempfile.gettempdir(), name)

SOCK = _sock_path()

TOOLS = [
    {"name": "whisp_status", "description": "Recording state (recording bool, active mode). Use before/after toggle.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_toggle", "description": "Toggle dictation like Alt+Space: starts recording or stops+transcribes+pastes.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_start", "description": "Start recording (popup opens, waveform live).",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_stop", "description": "Stop, transcribe, refine and paste. Returns transcript + final text. SLOW (STT+LLM).",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_cancel", "description": "Discard the current recording without transcribing.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_set_mode", "description": "Switch dictation mode by id (message, super, mail, note, voice, meeting...).",
     "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}},
    {"name": "whisp_list_modes", "description": "List modes with their STT engine (local/cloud) and LLM.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_list_mics", "description": "List capture devices (cpal names). Empty/absent app = app not running.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_get_config", "description": "App config: sttModel, language, inputDevice, theme, popupPos.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "whisp_history", "description": "Recent dictations with raw transcript + refined text (opt-in, contenu sensible).",
     "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "default": 5}}}},
    {"name": "whisp_set_config", "description": "Update app config. Whitelist sûre : popupPos, sttModel, theme, language, inputDevice, agent, agentContext, realtimePreview. agentCommand/agentCwd = UI uniquement.",
     "inputSchema": {"type": "object", "properties": {"popupPos": {"type": "string"}, "sttModel": {"type": "string"}, "theme": {"type": "string"}, "language": {"type": "string"}, "inputDevice": {"type": "string"}, "agent": {"type": "string"}, "agentContext": {"type": "boolean"}, "realtimePreview": {"type": "boolean"}}}},
    {"name": "whisp_set_stt", "description": "Set a mode's transcription engine: stt local (offline) or cloud_fish_audio (Fish cloud, fast). Persists as custom mode.",
     "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}, "stt": {"type": "string"}}, "required": ["id", "stt"]}},
    {"name": "whisp_chat", "description": "Chat via le provider configuré (claude = abonnement OAuth + Style, défaut). Style appliqué côté backend.",
     "inputSchema": {"type": "object", "properties": {"message": {"type": "string"}, "provider": {"type": "string", "default": "claude"}}, "required": ["message"]}},
]


def sock_cmd(payload: dict) -> dict:
    """Send one JSON command, return parsed reply (or error dict)."""
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(120)
        s.connect(SOCK)
    except OSError as e:
        return {"ok": False, "error": f"whisp app not reachable ({SOCK}): {e}. Launch the app first."}
    try:
        f = s.makefile("rw")
        f.write(json.dumps(payload) + "\n")
        f.flush()
        line = f.readline()
        if not line:
            return {"ok": False, "error": "empty reply from whisp app"}
        return json.loads(line)
    except (OSError, ValueError) as e:
        return {"ok": False, "error": f"socket error: {e}"}
    finally:
        try:
            s.close()
        except OSError:
            pass


def tool_text(result: dict) -> dict:
    return {"content": [{"type": "text", "text": json.dumps(result, ensure_ascii=False, indent=1)}]}


def handle(req: dict):
    mid = req.get("id")
    method = req.get("method")
    params = req.get("params", {})
    # MCP envoie `arguments` (pas `args`).
    args = params.get("arguments", params.get("args", {})) or {}

    def reply(result=None, error=None):
        # Notifications (pas d'id) : jamais de réponse.
        if mid is None:
            return
        out = {"jsonrpc": "2.0", "id": mid}
        if error is not None:
            out["error"] = error
        else:
            out["result"] = result if result is not None else {}
        sys.stdout.write(json.dumps(out) + "\n")
        sys.stdout.flush()

    if method == "initialize":
        # Renvoyer la version du client, pas une version en dur.
        client_ver = params.get("protocolVersion", "2024-11-05")
        reply({"protocolVersion": client_ver,
               "capabilities": {"tools": {}},
               "serverInfo": {"name": "whisp", "version": "0.1.0"}})
    elif method == "ping":
        reply({})
    elif method and method.startswith("notifications/"):
        pass  # notifications : pas de réponse, jamais d'erreur
    elif method == "tools/list":
        reply({"tools": TOOLS})
    elif method == "tools/call":
        name = params.get("name", "")
        mapping = {
            "whisp_status": ({"cmd": "status"}, None),
            "whisp_toggle": ({"cmd": "toggle"}, None),
            "whisp_start": ({"cmd": "start"}, None),
            "whisp_stop": ({"cmd": "stop"}, None),
            "whisp_cancel": ({"cmd": "cancel"}, None),
            "whisp_set_mode": ({"cmd": "set_mode", "id": args.get("id", "")}, None),
            "whisp_list_modes": ({"cmd": "list_modes"}, None),
            "whisp_list_mics": ({"cmd": "list_mics"}, None),
            "whisp_get_config": ({"cmd": "get_config"}, None),
            # Agent retiré de la whitelist MCP : agentCommand/agentCwd = exécution
            # de commande arbitraire via prompt injection. UI uniquement.
            "whisp_set_config": ({"cmd": "set_config", **{k: v for k, v in args.items() if k in ("popupPos", "sttModel", "theme", "language", "inputDevice", "agent", "agentContext", "realtimePreview", "chatProvider", "chatModel", "claudeStyle")}}, None),
            "whisp_set_stt": ({"cmd": "set_stt", "id": args.get("id", ""), "stt": args.get("stt", "")}, None),
            "whisp_history": ({"cmd": "history", "limit": args.get("limit", 5)}, None),
            "whisp_chat": ({"cmd": "chat", "provider": args.get("provider", "claude") or "claude",
                            "messages": [{"role": "user", "content": args.get("message", "")}]}, None),
        }
        if name not in mapping:
            reply(error={"code": -32602, "message": f"unknown tool '{name}'"})
            return
        payload, _ = mapping[name]
        res = sock_cmd(payload)
        # isError quand le backend répond ok:false (sinon l'agent croit que ça a marché).
        if isinstance(res, dict) and res.get("ok") is False:
            reply({"content": [{"type": "text", "text": json.dumps(res, ensure_ascii=False)}], "isError": True})
        else:
            reply(tool_text(res))
    else:
        # Sans id (notification inconnue) : silence, pas d'erreur.
        if mid is None:
            return
        reply(error={"code": -32601, "message": f"unknown method '{method}'"})

def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            handle(json.loads(line))
        except ValueError as e:
            sys.stdout.write(json.dumps(
                {"jsonrpc": "2.0", "id": None,
                 "error": {"code": -32700, "message": f"parse: {e}"}}) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
