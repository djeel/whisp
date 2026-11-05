// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "toggle" {
        // Ping a running instance over the local socket; exit 0 on delivery.
        // Même chemin que le daemon (XDG_RUNTIME_DIR d'abord, /tmp fallback).
        let name = format!("whisp-{}.sock", libc_uid());
        let sock = std::env::var("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .ok()
            .filter(|d: &std::path::PathBuf| d.is_dir())
            .map(|d| d.join(&name))
            .unwrap_or(std::env::temp_dir().join(&name));
        use std::io::Write;
        match std::os::unix::net::UnixStream::connect(&sock) {
            Ok(mut s) => {
                let _ = s.write_all(b"toggle\n");
                return;
            }
            Err(e) => {
                eprintln!("whisp: no running instance ({e})");
                std::process::exit(1);
            }
        }
    }
    whisp_lib::run()
}

#[cfg(unix)]
fn libc_uid() -> u32 {
    // Avoid pulling libc crate: read /proc/self/loginuid fallback to UID env.
    std::env::var("UID")
        .ok()
        .and_then(|u| u.parse().ok())
        .unwrap_or(1000)
}

#[cfg(not(unix))]
fn libc_uid() -> u32 {
    1000
}
