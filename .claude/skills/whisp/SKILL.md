# whisp — dictée vocale + Claude abonnement

Pilote whisp (dictée locale / Fish cloud) et discute via le compte Claude
(abonnement OAuth + Style) depuis n'importe quel agent.

## Connexion Claude (une fois)

```bash
claude setup-token   # → sk-ant-oat-…
```

Coller le token dans whisp : Settings → Claude → Connecter.
Vérifier : `has_api_key(claude)` ou tool `whisp_chat`.

## Style Claude (ton préréglé)

Coller le texte du Style (claude.ai → Styles → Edit) dans
Settings → Claude → Style. Injecté en `system` pour `claude` + `anthropic`
(chat, voix, raffinage dictée). Vide = pas de bloc system.

## MCP (13 tools)

Serveur : `mcp_server.py` (stdio, branché via `.mcp.json`).
L'app doit tourner (socket `/run/user/<uid>/whisp-<uid>.sock`).

| Tool | Usage |
|---|---|
| `whisp_status` | état recording + mode actif (avant/après toggle) |
| `whisp_toggle` | Alt+Espace : start ou stop+transcribe+paste |
| `whisp_start` / `whisp_stop` / `whisp_cancel` | contrôle fin |
| `whisp_set_mode` | changer de mode (`id`) |
| `whisp_list_modes` | modes + moteur STT/LLM |
| `whisp_set_stt` | `local` \| `cloud_fish_audio` par mode |
| `whisp_list_mics` / `whisp_get_config` | micros, config |
| `whisp_set_config` | `chatProvider`, `chatModel`, `claudeStyle`, + whitelist sûre (jamais `staCommand`/`staCwd` = exécution arbitraire) |
| `whisp_history` | dernières dictées (contenu sensible, opt-in) |
| `whisp_chat` | chat via provider (`claude` défaut, Style appliqué backend) |

## Règles

- `whisp_stop` est LENT (STT+LLM) : relire le résultat via `whisp_history`.
- `whisp_chat` sans compte connecté répond `ok:false` → guider vers `claude setup-token`.
- Token OAuth : jamais en config, jamais loggué, jamais via MCP.
- `set_stt cloud` sans clé Fish retombe en local (normal).
