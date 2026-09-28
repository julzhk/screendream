fn main() {
    // screencapturekit's Swift bridge links against the system Swift runtime; link-args from
    // a dependency's build script don't reach our binaries, so add the rpath here.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_MACOS_CAPTURE").is_some()
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
