/** Shim : `fetchAppConfig` vit dans `./tauri` (single-flight + queue partagés).
 * Les imports existants restent valides ; le code neuf importe de `./tauri`. */
export { fetchAppConfig } from "./tauri";
