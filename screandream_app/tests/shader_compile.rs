//! Parses and validates the WGSL shaders with naga, the same front end wgpu uses at runtime.

fn validate(name: &str, source: &str) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{name}: {error:?}"));
}

#[test]
fn visualizer_shader_is_valid_wgsl() {
    validate(
        "visualizer.wgsl",
        include_str!("../src/render/shaders/visualizer.wgsl"),
    );
}

#[test]
fn post_process_shader_is_valid_wgsl() {
    validate(
        "post_process.wgsl",
        include_str!("../src/render/shaders/post_process.wgsl"),
    );
}

#[test]
fn overlay_shader_is_valid_wgsl() {
    validate(
        "overlay.wgsl",
        include_str!("../src/render/shaders/overlay.wgsl"),
    );
}
