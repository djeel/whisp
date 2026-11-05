//! Scheme Caelestia : accent du recorder qui suit le fond d'écran.
//!
//! Source unique : `~/.local/state/caelestia/scheme.json` (écrit par le shell
//! Caelestia à chaque changement de wallpaper). Aucun spawn `caelestia` CLI :
//! lecture directe du JSON, `None` quand Caelestia est absent.
//!
//! Exposé au frontend via `caelestia_scheme` (+ `caelestia_watch` qui émet
//! `caelestia://scheme` quand le fichier change, pour un suivi live sans poll).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Deserialize)]
struct SchemeFile {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    colours: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaelestiaScheme {
    pub available: bool,
    pub mode: String,
    pub primary: String,
    pub on_primary: String,
    pub surface: String,
    pub on_surface: String,
    pub background: String,
}

fn scheme_path() -> std::path::PathBuf {
    match dirs::home_dir() {
        Some(h) => h.join(".local/state/caelestia/scheme.json"),
        None => std::path::PathBuf::from("/tmp/scheme.json"),
    }
}

fn hex6(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", t.to_lowercase()))
    } else {
        None
    }
}

/// Lit le scheme Caelestia. `available=false` quand absent/illisible.
#[tauri::command]
pub fn caelestia_scheme() -> CaelestiaScheme {
    let fallback = CaelestiaScheme {
        available: false,
        mode: "dark".to_string(),
        primary: "#f59e0b".to_string(),
        on_primary: "#1a1206".to_string(),
        surface: "#1e1e1e".to_string(),
        on_surface: "#e8e8e8".to_string(),
        background: "#141414".to_string(),
    };
    let text = match std::fs::read_to_string(scheme_path()) {
        Ok(t) => t,
        Err(_) => return fallback,
    };
    let parsed: SchemeFile = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(_) => return fallback,
    };
    if parsed.colours.is_empty() {
        return fallback;
    }
    scheme_from_parsed(&parsed)
}

/// Construit le scheme depuis le JSON parsé (`available=true`).
/// Fonction pure — testée sans toucher `~/.local/state/…`.
fn scheme_from_parsed(parsed: &SchemeFile) -> CaelestiaScheme {
    let pick = |k: &str, fb: &str| {
        parsed
            .colours
            .get(k)
            .and_then(|v| hex6(v))
            .unwrap_or_else(|| fb.to_string())
    };
    let mode = if parsed.mode == "light" {
        "light"
    } else {
        "dark"
    };
    CaelestiaScheme {
        available: true,
        mode: mode.to_string(),
        primary: pick("primary", "#f59e0b"),
        on_primary: pick(
            "onPrimary",
            if mode == "light" {
                "#ffffff"
            } else {
                "#1a1206"
            },
        ),
        surface: pick(
            "surface",
            if mode == "light" {
                "#f5f5f5"
            } else {
                "#1e1e1e"
            },
        ),
        on_surface: pick(
            "onSurface",
            if mode == "light" {
                "#1a1a1a"
            } else {
                "#e8e8e8"
            },
        ),
        background: pick(
            "background",
            if mode == "light" {
                "#efefef"
            } else {
                "#141414"
            },
        ),
    }
}

static WATCHING: AtomicBool = AtomicBool::new(false);

/// Surveille `scheme.json` et émet `caelestia://scheme` QUAND le contenu
/// change (hash du fichier). Idempotent : un seul watcher par process ;
/// le frontend appelle une fois au mount.
///
/// Poll 2 s conservé (pas de dep `notify`) : la lecture + hash fait
/// ~microsecondes (fichier < 4 Ko) et l'émission est évitée tant que le
/// contenu est identique — réécriture à mtime égal (éditeur qui touche le
/// fichier sans changer le wallpaper) = zéro event, zéro re-render.
#[tauri::command]
pub fn caelestia_watch(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;
    if WATCHING.swap(true, Ordering::SeqCst) {
        return Ok("already watching".to_string());
    }
    std::thread::spawn(move || {
        // Hash initial : évite une émission au mount (le frontend a déjà
        // appelé `caelestia_scheme` et peint avec).
        let mut last_hash = file_hash();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let hash = file_hash();
            // Fichier absent/illisible : rien à comparer, on réessaie (le
            // shell Caelestia peut redémarrer et recréer le fichier).
            let (old, new) = match (&last_hash, &hash) {
                (Some(o), Some(n)) => (o, n),
                _ => {
                    last_hash = hash;
                    continue;
                }
            };
            if old != new {
                last_hash = hash;
                let scheme = caelestia_scheme();
                let _ = app.emit("caelestia://scheme", &scheme);
            }
        }
    });
    Ok("watching".to_string())
}

/// Hash FNV-1a 64 bits du contenu de `scheme.json` (`None` si illisible).
/// Raison : `u64` copiable comparé sans allocation, là où le `String`
/// précédent copiait tout le fichier à chaque poll (2 copies/s en continu).
fn file_hash() -> Option<u64> {
    let bytes = std::fs::read(scheme_path()).ok()?;
    // FNV-1a : 1 passe, 0 alloc, ~ns sur < 4 Ko — largement assez pour
    // détecter un changement (pas besoin de crypto ici).
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex6_accepts_plain_and_hash() {
        assert_eq!(hex6("a9cae8"), Some("#a9cae8".to_string()));
        assert_eq!(hex6("#A9CAE8"), Some("#a9cae8".to_string()));
        assert_eq!(hex6("xyz"), None);
        assert_eq!(hex6("#12345"), None);
    }

    #[test]
    fn scheme_from_parsed_maps_colours_and_mode() {
        // JSON réel du shell Caelestia (clés camelCase, hex sans `#`).
        let parsed: SchemeFile = serde_json::from_str(
            r#"{"mode":"light","colours":{"primary":"a9cae8","onPrimary":"ffffff","surface":"zz","onSurface":"1a1a1a","background":"efefef"}}"#,
        )
        .unwrap();
        let s = scheme_from_parsed(&parsed);
        assert!(s.available);
        assert_eq!(s.mode, "light");
        assert_eq!(s.primary, "#a9cae8"); // normalisé + `#`
        assert_eq!(s.on_primary, "#ffffff");
        assert_eq!(s.surface, "#f5f5f5"); // "zz" invalide → fallback clair
        assert_eq!(s.on_surface, "#1a1a1a");
        assert_eq!(s.background, "#efefef");
    }
}
