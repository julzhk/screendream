fn main() {
    // The bundled mesh is loaded from `assets/`. Inside a macOS .app bundle it lives in
    // `Contents/Resources/` next to `Contents/MacOS/<exe>`; otherwise fall back to the crate
    // root when launched from elsewhere (e.g. `cargo run --manifest-path ...`).
    if !std::path::Path::new("assets/default-scene.ply").exists() {
        let bundle_resources = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources")))
            .filter(|dir| dir.join("assets/default-scene.ply").exists());
        let _ = match bundle_resources {
            Some(dir) => std::env::set_current_dir(dir),
            None => std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")),
        };
    }

    #[cfg(feature = "gpu-render")]
    if let Err(error) = screendream::app::run() {
        eprintln!("Screendream could not start: {error}");
        std::process::exit(1);
    }

    #[cfg(not(feature = "gpu-render"))]
    eprintln!("Screendream was built without the `gpu-render` feature; nothing to display.");
}
