// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Client CLI léger : `whisp toggle|start|stop|cycle-mode|status` → socket unix.
//! Utilisé par les binds Hyprland natifs (le plugin Tauri global-shortcut
//! n'a pas de backend Wayland : seul le compositor intercepte globalement).
//! Binaire séparé de la lib (`whisp_lib::run`) : pas de Tauri ici, juste std.

const CMDS: &[&str] = &[
    "toggle",
    "open",
    "start",
    "stop",
    "cycle-mode",
    "status",
    "set_mode",
    "list_modes",
    "history",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && CMDS.contains(&args[1].as_str()) {
        let cmd = &args[1];
        let line = build_line(cmd, &args[2..]);
        send_line(&line);
        return;
    }
    whisp_lib::run()
}

/// Construit la ligne protocole : `toggle` nu (compat backend sans token),
/// tout le reste en JSON + token (cf. socket::verify_token côté backend).
/// `cycle-mode` (CLI, avec tiret) est mappé vers `cycle_mode` (socket).
fn build_line(cmd: &str, rest: &[String]) -> String {
    let cmd = if cmd == "cycle-mode" {
        "cycle_mode"
    } else {
        cmd
    };
    if cmd == "toggle" {
        return "toggle\n".to_string();
    }
    let tok = read_token();
    let mut extra = String::new();
    if cmd == "set_mode" {
        if let Some(id) = rest.first() {
            extra = format!(",\"id\":{}", json_str(id));
        }
    } else if cmd == "history" {
        if let Some(n) = rest.first() {
            // Clamp : évite d'inonder le terminal (le backend clamp déjà à 100).
            let n: u32 = n.parse().unwrap_or(10).min(100);
            extra = format!(",\"limit\":{n}");
        }
    }
    format!("{{\"cmd\":\"{cmd}\",\"token\":\"{tok}\"{extra}}}\n")
}

/// Token partagé (`~/.config/whisp/socket.token`, 0600). Absent = chaîne
/// vide : le backend autorise une fois puis crée le token (first boot).
fn read_token() -> String {
    let path = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .filter(|d| d.is_absolute())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".config"))
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
        })
        .join("whisp/socket.token");
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Échappement JSON minimal pour une chaîne (guillemets + backslash).
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(unix)]
fn libc_uid() -> u32 {
    // libc dispo via les deps du workspace ; pas de lecture $UID (spoofable).
    #[allow(unsafe_code)]
    unsafe {
        libc::getuid()
    }
}

fn send_line(line: &str) {
    let name = format!("whisp-{}.sock", libc_uid());
    let sock = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .filter(|d: &std::path::PathBuf| d.is_absolute() && d.is_dir())
        .map(|d| d.join(&name))
        .unwrap_or(std::env::temp_dir().join(&name));
    use std::io::{Read, Write};
    use std::time::Duration;
    let conn = std::os::unix::net::UnixStream::connect(&sock);
    let mut s = match conn {
        Ok(s) => s,
        Err(e) => {
            eprintln!("whisp: no running instance ({e})");
            std::process::exit(1);
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = s.write_all(line.as_bytes());
    // Signale fin d'écriture : le serveur répond puis garde la connexion
    // (keep-alive) — sans shutdown, read() bloquerait.
    let _ = s.shutdown(std::net::Shutdown::Write);
    // Lit la réponse (utile pour `status`, ignorée sinon).
    let mut buf = [0u8; 8192];
    match s.read(&mut buf) {
        Ok(n) if n > 0 => {
            let out = String::from_utf8_lossy(&buf[..n]);
            print!("{out}");
            if out.contains(r#""ok":false"#) {
                std::process::exit(1);
            }
        }
        _ => {}
    }
}

#[cfg(not(unix))]
fn libc_uid() -> u32 {
    1000
}
