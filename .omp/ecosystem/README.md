# Écosystème de travail OMP — protocole (1 page)

Racine projet : `<repo>/.omp/ecosystem/` · Racine globale (hors-projet) : `~/.omp/agent/ecosystem/`.
`claims/`, `channels/`, `dm/`, `presence/` sont NON versionnés (voir `.gitignore`) ;
`README.md` + `.gitkeep` sont versionnés.

## Nommage

- Session : `ECO_SELF = sess-<hostname>-<pid>-<YYYYMMDDTHHMMSSZ>` (`[A-Za-z0-9-]`).
- Présence : `presence/<ECO_SELF>.json` —
  `{"ecoId","hostname","pid","cwd","startedAt","lastBeat","currentTask"}` (ISO, `currentTask` ≤ 80 car).
  Beat 60 s, TTL 180 s : `now - lastBeat > 180 s` = parti (masqué, pas supprimé).
- Réservation : `claims/<rel-avec-__-au-lieu-de-/>.json` (ex. `src/foo.ts` → `src__foo.ts.json`).
  `{"owner":"<ECO_SELF>/<agentId>","paths":["<rel>"],"intent":"≤140 car",` \
  `"createdAt":"ISO","expiresAt":"ISO = createdAt+600s","mode":"exclusive"}`.
  Préfixe dossier : suffixe `/*` uniquement. Rejeté : absolu, `..`, `:`, `?`, `#`, hors repo.
- Message : `channels/<chan>/<UTC-basic>_<ECO_SELF>.md`, DM : `dm/<dest-eco>/<UTC-basic>_<ECO_SELF>.md`.
  `<chan>` kebab-case ≤ 32 car. Horodatage : `date -u +%Y%m%dT%H%M%SZ`. Corps ≤ 64 KiB :
  ```
  from: <ECO_SELF>/<agentId>
  chan: <chan>            # ou : to: <dest-eco> pour DM
  ts: <ISO>
  reply-to: <fichier|->

  <Markdown libre>
  ```

## Règles

1. **Claim avant edit** : `read` claim + `glob claims/*` (préfixe couvrant) avant tout
   `edit`/`write` sur l'arbre partagé. Absent → `write` claim puis éditer.
   Valide + autre owner → INTERDIT : demander dans `#work` + DM, `wait` (cap 30 min).
   Expiré depuis > 60 s → takeover : écraser + `took-over from <owner>` dans `#work` + DM.
   Heartbeat `expiresAt = now+600s` toutes les 300 s. Libération : `bash rm claims/<f>.json`
   (ou écraser `{"released":true}` si `rm` indisponible = absent).
   Exemptés : `local://`, `/tmp`, `artifact://` (on ne réserve que ce qu'un pair peut écraser).
2. **Salon** : écrire = un `write` atomique ; lire = `glob` trié + `read` après curseur
   `local://eco-cursor-<chan>.txt`. Jamais `agent://all` pour l'inter-sessions (intra-session only).
3. **Relay** : un `EcoRelay` par session (`task` batch), poll 10 s, heartbeat 60 s,
   `yield` résumé si nouveau sinon `{"quiet":true}`. Relance avec curseurs `local://eco-cursor-*`.
4. **Prune** (relay, 1×/h) : `bash rm` messages/DM > 30 j, `presence/*` avec `now-lastBeat > 1 h`.
   Jamais `README.md`/`.gitkeep`. Échec `rm` → log `#work`, pas fatal.
5. **Lecteurs robustes** : répertoire absent = vide ; fichier illisible / > 1 MiB = ignoré + 1 ligne de log.

## Exemples

```sh
date -u +%Y%m%dT%H%M%SZ                      # horodatage message
ECO_SELF=sess-alex-4211-20261105T090102Z    # identité (local://eco-self.txt)
```

```
claims/src__flag.txt.json        ← réservation src/flag.txt
channels/work/20261105T090512Z_sess-alex-4211-20261105T090102Z.md
dm/sess-alex-4211-20261105T090102Z/20261105T090613Z_sess-sam-...md
```

Écart éventuel : si ce repo interdit tout `.omp` versionné, tout le dossier est ignoré
sauf ce `README.md` — noté ici le cas échéant : aucun écart (layout standard ci-dessus).
