//! Appearance backend: recorder popup size presets.
//!
//! `set_popup_size` resizes the `recorder` window (S/M/L → 512/576/704 px
//! wide, 200 px tall). Hauteur inchangée : seule la largeur est réduite (-20 %).
//! are applied frontend-side (CSS var + recorder localStorage) and only
//! persisted through `set_config`.

pub const SIZES: &[(&str, u32, u32)] = &[("S", 512, 200), ("M", 576, 200), ("L", 704, 200)];

/// Resize the recorder popup. `size` is `S`, `M` or `L`.
#[tauri::command]
pub fn set_popup_size(app: tauri::AppHandle, size: String) -> Result<String, String> {
    use tauri::Manager;
    let (_, w, h) = SIZES
        .iter()
        .find(|(id, _, _)| *id == size.trim().to_uppercase())
        .ok_or_else(|| format!("unknown popup size '{size}' (S|M|L)"))?;
    let win = app
        .get_webview_window("recorder")
        .ok_or_else(|| "recorder window not found".to_string())?;
    // Ne changer que la largeur : la hauteur suit le contenu via l'autosize
    // frontend (forcer 200 px re-coupait le bas après un partial/une erreur).
    let scale = win.scale_factor().unwrap_or(1.0);
    let cur_h = win
        .inner_size()
        .map(|s| f64::from(s.height) / scale)
        .unwrap_or(200.0);
    win.set_size(tauri::LogicalSize {
        width: f64::from(*w),
        height: cur_h,
    })
    .map_err(|e| format!("resize recorder: {e}"))?;
    Ok(format!("Popup {size}: {w}×{h}."))
}
