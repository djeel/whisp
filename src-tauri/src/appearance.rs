//! Appearance backend: recorder popup size presets.
//!
//! `set_popup_size` resizes the `recorder` window (S/M/L → 640/720/880 px
//! wide, 200 px tall). Opacity (`card_opacity`) and Mochi (`popup_mochi`)
//! are applied frontend-side (CSS var + recorder localStorage) and only
//! persisted through `set_config`.

pub const SIZES: &[(&str, u32, u32)] =
    &[("S", 640, 200), ("M", 720, 200), ("L", 880, 200)];

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
    win.set_size(tauri::LogicalSize {
        width: f64::from(*w),
        height: f64::from(*h),
    })
    .map_err(|e| format!("resize recorder: {e}"))?;
    Ok(format!("Popup {size}: {w}×{h}."))
}

