fn main() {
    // The bundled mesh is loaded from `assets/`; fall back to the crate root when launched
    // from elsewhere (e.g. `cargo run --manifest-path ...`).
    if !std::path::Path::new("assets/default-scene.ply").exists() {
        let _ = std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"));
    }

    #[cfg(feature = "gpu-render")]
    if let Err(error) = screendream::app::run() {
        eprintln!("Screendream could not start: {error}");
        std::process::exit(1);
    }

    #[cfg(not(feature = "gpu-render"))]
    eprintln!("Screendream was built without the `gpu-render` feature; nothing to display.");
}
