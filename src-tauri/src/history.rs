//! Local dictation history (SQLite) for whisp.
//!
//! Database: `~/.local/share/whisp/history.db`, table `history`
//! `(id, timestamp, mode, transcript_raw, text_final, audio_path)`.
//! Audio WAVs (16 kHz mono) are kept under `~/.local/share/whisp/audio/`
//! and removed when their entry is deleted.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

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

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        mode: row.get(2)?,
        transcript_raw: row.get(3)?,
        text_final: row.get(4)?,
        audio_path: row.get(5)?,
    })
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
    let entry = HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        mode: mode.to_string(),
        transcript_raw: transcript_raw.to_string(),
        text_final: text_final.to_string(),
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
    Ok(HistoryEntry { audio_path, ..entry })
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
    let conn = connect()?;
    let like = format!("%{query}%");
    let rows: Vec<HistoryEntry> = conn
        .prepare(
            "SELECT id, timestamp, mode, transcript_raw, text_final, audio_path FROM history
             WHERE transcript_raw LIKE ?1 OR text_final LIKE ?1 OR mode LIKE ?1
             ORDER BY timestamp DESC LIMIT 100",
        )
        .map_err(|e| format!("search history: {e}"))?
        .query_map(params![like], row_to_entry)
        .map_err(|e| format!("search history: {e}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("search history: {e}"))?;
    Ok(rows)
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
            params![text_final, id],
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
        total_words += text.split_whitespace().count() as u64;
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
