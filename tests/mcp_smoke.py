#!/usr/bin/env python3
"""Smoke test MCP (sans l'app) : framing JSON-RPC, tools/list, routage.

Lance mcp_server.py en subprocess, dialogue en JSON-RPC sur stdin/stdout.
Sans app en cours, les tools/call doivent répondre isError (socket
injoignable) — jamais de crash, jamais de timeout. Zéro dépendance.
"""
import json
import os
import subprocess
import sys

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MCP = os.path.join(APP_DIR, "mcp_server.py")

EXPECTED_TOOLS = {
    "whisp_status", "whisp_toggle", "whisp_start", "whisp_stop",
    "whisp_cancel", "whisp_set_mode", "whisp_list_modes", "whisp_list_mics",
    "whisp_get_config", "whisp_history", "whisp_repaste", "whisp_undo_paste",
    "whisp_health", "whisp_set_config", "whisp_set_stt", "whisp_chat",
}


def rpc(proc, method, params=None, mid=1):
    req = {"jsonrpc": "2.0", "id": mid, "method": method, "params": params or {}}
    proc.stdin.write(json.dumps(req) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())


def main() -> int:
    proc = subprocess.Popen(
        [sys.executable, MCP],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        text=True, bufsize=1,
        env={**os.environ, "XDG_RUNTIME_DIR": "/tmp/whisp-smoke-none"},
    )
    fails = []

    def check(name, cond, extra=""):
        print(("PASS " if cond else "FAIL ") + name + (f" ({extra})" if extra and not cond else ""))
        if not cond:
            fails.append(name)

    try:
        r = rpc(proc, "initialize", {"protocolVersion": "2024-11-05"})
        check("initialize", r.get("result", {}).get("protocolVersion") == "2024-11-05", str(r))

        r = rpc(proc, "ping", {}, mid=2)
        check("ping", r.get("result") == {}, str(r))

        r = rpc(proc, "tools/list", {}, mid=3)
        got = {t["name"] for t in r.get("result", {}).get("tools", [])}
        check("tools/list", EXPECTED_TOOLS <= got, f"missing={EXPECTED_TOOLS - got}")

        # Sans app : routage OK mais isError (socket injoignable), pas de crash.
        r = rpc(proc, "tools/call",
                {"name": "whisp_health", "arguments": {}}, mid=4)
        check("call/health sans app -> isError",
              r.get("result", {}).get("isError") is True, str(r)[:200])

        r = rpc(proc, "tools/call",
                {"name": "whisp_nope", "arguments": {}}, mid=5)
        check("call/inconnu -> -32602",
              r.get("error", {}).get("code") == -32602, str(r)[:200])

        r = rpc(proc, "tools/call",
                {"name": "whisp_undo_paste", "arguments": {}}, mid=6)
        check("call/undo_paste sans app -> isError",
              r.get("result", {}).get("isError") is True, str(r)[:200])
    finally:
        proc.kill()
        proc.wait()

    print(f"{len(fails)} échec(s)" if fails else "smoke MCP OK")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
