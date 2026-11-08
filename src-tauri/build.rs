fn main() {
    // Le frontend embarqué DOIT suivre `dist/` : sans ça, cargo réutilise
    // le contexte Tauri en cache (assets périmés → page blanche / vieille UI
    // alors que `dist/` est frais). `tauri_build` n'émet pas ce rerun.
    println!("cargo:rerun-if-changed=../dist");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
