//! Local dictation history (SQLite) for whisp.
//!
//! Database: `~/.local/share/whisp/history.db`, table `history`
//! `(id, timestamp, mode, transcript_raw, text_final, audio_path)`.
//! Audio WAVs (16 kHz mono) are kept under `~/.local/share/whisp/audio/`
//! and removed when their entry is deleted.
//!
//! Textes chiffrés au repos : `transcript_raw` et `text_final` sont stockés
//! `whisp1:base64(nonce24 || chiffré)` (XChaCha20-Poly1305, clé machine =
//! SHA256(machine-id)). Lignes legacy en clair lues telles quelles puis
//! re-chiffrées à la prochaine insertion (`migrate_legacy`).

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, NewAead},
    Key, XChaCha20Poly1305, XNonce,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Préfixe des cellules chiffrées (le reste est legacy en clair).
const CRYPT_PREFIX: &str = "whisp1:";

/// Clé machine mise en cache (SHA256(machine-id || "whisp-history-v1")).
static HISTORY_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

fn machine_id_bytes() -> Result<Vec<u8>, String> {
    for p in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(s) = std::fs::read_to_string(p) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Ok(s.into_bytes());
            }
        }
    }
    // Repli stable (jamais vide en pratique sur un poste Linux).
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    if host.trim().is_empty() && user.trim().is_empty() {
        return Err("aucun identifiant machine stable (historique non chiffrable)".to_string());
    }
    Ok(format!("{}:{user}", host.trim()).into_bytes())
}

fn history_key() -> Result<[u8; 32], String> {
    if let Some(k) = HISTORY_KEY.get() {
        return Ok(*k);
    }
    let mut h = Sha256::new();
    h.update(machine_id_bytes()?);
    h.update(b"whisp-history-v1");
    let out = h.finalize();
    let mut k = [0u8; 32];
    k.copy_from_slice(&out);
    let _ = HISTORY_KEY.set(k);
    Ok(k)
}

fn random_nonce() -> Result<[u8; 24], String> {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| format!("urandom: {e}"))?;
    let mut n = [0u8; 24];
    f.read_exact(&mut n).map_err(|e| format!("urandom: {e}"))?;
    Ok(n)
}

fn encrypt_with(key: &[u8; 32], plain: &str) -> Result<String, String> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = random_nonce()?;
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), plain.as_bytes())
        .map_err(|e| format!("chiffrement: {e}"))?;
    let mut blob = Vec::with_capacity(24 + ct.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ct);
    Ok(format!("{CRYPT_PREFIX}{}", B64.encode(&blob)))
}

fn decrypt_with(key: &[u8; 32], stored: &str) -> Result<String, String> {
    let b64 = stored
        .strip_prefix(CRYPT_PREFIX)
        .ok_or_else(|| "cellule non chiffrée".to_string())?;
    let blob = B64
        .decode(b64)
        .map_err(|e| format!("base64 history: {e}"))?;
    if blob.len() < 24 + 16 {
        return Err("blob history trop court".to_string());
    }
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let pt = cipher
        .decrypt(XNonce::from_slice(&blob[..24]), &blob[24..])
        .map_err(|_| "clé machine différente ?".to_string())?;
    String::from_utf8(pt).map_err(|e| format!("utf8 history: {e}"))
}

fn encrypt_str(plain: &str) -> Result<String, String> {
    encrypt_with(&history_key()?, plain)
}

/// Déchiffre, ou laisse passer le legacy en clair. Une cellule préfixée
/// mais illisible (DB copiée d'une autre machine) affiche un marqueur
/// au lieu du blob base64.
fn plain(stored: &str) -> String {
    if !stored.starts_with(CRYPT_PREFIX) {
        return stored.to_string();
    }
    decrypt_with(&history_key().unwrap_or([0u8; 32]), stored)
        .unwrap_or_else(|_| "[dictée illisible : base copiée d'une autre machine ?]".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub mode: String,
    pub transcript_raw: String,
    pub text_final: String,
    pub audio_path: Option<String>,
}

fn db_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("whisp")
        .join("history.db")
}

fn audio_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("whisp")
        .join("audio")
}

fn connect() -> Result<Connection, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir history: {e}"))?;
    }
    let conn = Connection::open(&path).map_err(|e| format!("open history db: {e}"))?;
    // Anti-freeze : abandonne au lieu de bloquer si la DB est verrouillée.
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("busy timeout: {e}"))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            mode TEXT NOT NULL,
            transcript_raw TEXT NOT NULL,
            text_final TEXT NOT NULL,
            audio_path TEXT
        )",
        [],
    )
    .map_err(|e| format!("migrate history db: {e}"))?;
    Ok(conn)
}

fn row_to_entry(row: &Row) -> rusqlite::Result<HistoryEntry> {
    // Textes déchiffrés à la lecture (legacy en clair accepté, cf. `plain`).
    let transcript_raw: String = row.get(3)?;
    let text_final: String = row.get(4)?;
    Ok(HistoryEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        mode: row.get(2)?,
        transcript_raw: plain(&transcript_raw),
        text_final: plain(&text_final),
        audio_path: row.get(5)?,
    })
}

/// Re-chiffre les lignes legacy en clair (une fois, à la prochaine insertion).
/// Les cellules déjà préfixées sont laissées intactes.
fn migrate_legacy(conn: &Connection) -> Result<(), String> {
    let legacy: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT id, transcript_raw, text_final FROM history
             WHERE transcript_raw NOT LIKE 'whisp1:%' OR text_final NOT LIKE 'whisp1:%'
             LIMIT 500",
        )
        .map_err(|e| format!("migrate history: {e}"))?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("migrate history: {e}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("migrate history: {e}"))?;
    for (id, raw, fin) in &legacy {
        let enc_raw = if raw.starts_with(CRYPT_PREFIX) {
            raw.clone()
        } else {
            encrypt_str(raw)?
        };
        let enc_fin = if fin.starts_with(CRYPT_PREFIX) {
            fin.clone()
        } else {
            encrypt_str(fin)?
        };
        conn.execute(
            "UPDATE history SET transcript_raw = ?1, text_final = ?2 WHERE id = ?3",
            params![enc_raw, enc_fin, id],
        )
        .map_err(|e| format!("migrate history: {e}"))?;
    }
    Ok(())
}

/// Insert a new entry. When `wav_bytes` is `Some`, the audio is stored as
/// `{audio_dir}/{id}.wav` and linked via `audio_path`.
pub fn insert(
    mode: &str,
    transcript_raw: &str,
    text_final: &str,
    wav_bytes: Option<&[u8]>,
) -> Result<HistoryEntry, String> {
    let conn = connect()?;
    // Migration d'abord : si elle échoue, rien n'est inséré (état cohérent).
    migrate_legacy(&conn)?;
    let entry = HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        mode: mode.to_string(),
        transcript_raw: encrypt_str(transcript_raw)?,
        text_final: encrypt_str(text_final)?,
        audio_path: None,
    };
    let audio_path = match wav_bytes {
        Some(bytes) if !bytes.is_empty() => {
            let dir = audio_dir();
            std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir audio: {e}"))?;
            let path = dir.join(format!("{}.wav", entry.id));
            std::fs::write(&path, bytes).map_err(|e| format!("write audio: {e}"))?;
            Some(path.to_string_lossy().to_string())
        }
        _ => None,
    };
    conn.execute(
        "INSERT INTO history (id, timestamp, mode, transcript_raw, text_final, audio_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.id,
            entry.timestamp,
            entry.mode,
            entry.transcript_raw,
            entry.text_final,
            audio_path,
        ],
    )
    .map_err(|e| format!("insert history: {e}"))?;
    // Retourne le clair (la DB, elle, stocke le chiffré ci-dessus).
    Ok(HistoryEntry {
        transcript_raw: transcript_raw.to_string(),
        text_final: text_final.to_string(),
        audio_path,
        ..entry
    })
}

pub fn list(limit: u32) -> Result<Vec<HistoryEntry>, String> {
    let conn = connect()?;
    let rows: Vec<HistoryEntry> = conn
        .prepare("SELECT id, timestamp, mode, transcript_raw, text_final, audio_path FROM history ORDER BY timestamp DESC LIMIT ?1")
        .map_err(|e| format!("list history: {e}"))?
        .query_map(params![limit.max(1) as i64], row_to_entry)
        .map_err(|e| format!("list history: {e}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("list history: {e}"))?;
    Ok(rows)
}

pub fn search(query: &str) -> Result<Vec<HistoryEntry>, String> {
    // Recherche côté Rust : le SQL LIKE ne voit que du chiffré au repos.
    // (`list` déchiffre déjà via `row_to_entry`.)
    let q = query.to_lowercase();
    Ok(list(u32::MAX)?
        .into_iter()
        .filter(|e| {
            e.mode.to_lowercase().contains(&q)
                || e.transcript_raw.to_lowercase().contains(&q)
                || e.text_final.to_lowercase().contains(&q)
        })
        .take(100)
        .collect())
}

pub fn get(id: &str) -> Result<Option<HistoryEntry>, String> {
    let conn = connect()?;
    let mut stmt = conn
        .prepare("SELECT id, timestamp, mode, transcript_raw, text_final, audio_path FROM history WHERE id = ?1")
        .map_err(|e| format!("get history: {e}"))?;
    let mut rows = stmt
        .query_map(params![id], row_to_entry)
        .map_err(|e| format!("get history: {e}"))?;
    match rows.next() {
        Some(r) => Ok(Some(r.map_err(|e| format!("get history: {e}"))?)),
        None => Ok(None),
    }
}

/// Delete an entry and its stored WAV, if any.
pub fn delete(id: &str) -> Result<(), String> {
    if let Some(entry) = get(id)? {
        let conn = connect()?;
        conn.execute("DELETE FROM history WHERE id = ?1", params![id])
            .map_err(|e| format!("delete history: {e}"))?;
        if let Some(path) = entry.audio_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    } else {
        Err(format!("history entry '{id}' not found"))
    }
}

/// Replace `text_final` (used by reprocess with another mode/LLM).
pub fn update_final(id: &str, text_final: &str) -> Result<HistoryEntry, String> {
    let conn = connect()?;
    let changed = conn
        .execute(
            "UPDATE history SET text_final = ?1 WHERE id = ?2",
            params![encrypt_str(text_final)?, id],
        )
        .map_err(|e| format!("update history: {e}"))?;
    if changed == 0 {
        return Err(format!("history entry '{id}' not found"));
    }
    get(id)?.ok_or_else(|| format!("history entry '{id}' not found"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationStats {
    pub total_entries: u32,
    pub total_words: u64,
    pub today_entries: u32,
    pub avg_words: f64,
    pub minutes_saved: f64,
}

/// Aggregate stats for the Home stats cards: entry count, total word count
/// (from `text_final`), entries recorded today (local day), average words
/// per dictation, and estimated minutes saved at 150 words/min.
pub fn stats() -> Result<DictationStats, String> {
    let conn = connect()?;
    let mut stmt = conn
        .prepare("SELECT text_final, timestamp FROM history")
        .map_err(|e| format!("stats history: {e}"))?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("stats history: {e}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("stats history: {e}"))?;
    let today_start = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|d| d.and_local_timezone(chrono::Local).single())
        .map(|d| d.timestamp())
        .unwrap_or(0);
    let mut total_words: u64 = 0;
    let mut today_entries: u32 = 0;
    for (text, ts) in &rows {
        total_words += plain(text).split_whitespace().count() as u64;
        if *ts >= today_start {
            today_entries += 1;
        }
    }
    let total_entries = rows.len() as u32;
    let avg_words = if total_entries > 0 {
        total_words as f64 / total_entries as f64
    } else {
        0.0
    };
    Ok(DictationStats {
        total_entries,
        total_words,
        today_entries,
        avg_words,
        minutes_saved: total_words as f64 / 150.0,
    })
}

#[cfg(test)]
mod history_crypto_tests {
    use super::*;

    const KEY_A: [u8; 32] = [0xAB; 32];
    const KEY_B: [u8; 32] = [0xCD; 32];

    #[test]
    fn roundtrip_keeps_text_and_hides_it() {
        let stored = encrypt_with(&KEY_A, "bonjour le monde").expect("encrypt");
        assert!(stored.starts_with(CRYPT_PREFIX));
        assert!(!stored.contains("bonjour"));
        // Deux chiffrements du même clair diffèrent (nonce aléatoire).
        let again = encrypt_with(&KEY_A, "bonjour le monde").expect("encrypt");
        assert_ne!(stored, again);
        assert_eq!(decrypt_with(&KEY_A, &stored).expect("decrypt"), "bonjour le monde");
    }

    #[test]
    fn wrong_key_fails_closed() {
        let stored = encrypt_with(&KEY_A, "secret").expect("encrypt");
        assert!(decrypt_with(&KEY_B, &stored).is_err());
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        assert_eq!(plain("vieux texte en clair"), "vieux texte en clair");
    }
}
