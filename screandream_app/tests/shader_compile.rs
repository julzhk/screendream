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

/// Mode shaders are loaded with common.wgsl prepended, as `modes::shader_with_common` does.
fn validate_with_common(name: &str, source: &str) {
    let combined = format!(
        "{}\n{source}",
        include_str!("../src/render/shaders/common.wgsl")
    );
    validate(name, &combined);
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

#[test]
fn canyon_shader_is_valid_wgsl() {
    validate_with_common(
        "canyon.wgsl",
        include_str!("../src/render/shaders/canyon.wgsl"),
    );
}

#[test]
fn windmill_shader_is_valid_wgsl() {
    validate_with_common(
        "windmill.wgsl",
        include_str!("../src/render/shaders/windmill.wgsl"),
    );
}

#[test]
fn circuits_shader_is_valid_wgsl() {
    validate_with_common(
        "circuits.wgsl",
        include_str!("../src/render/shaders/circuits.wgsl"),
    );
}

#[test]
fn bubbles_shader_is_valid_wgsl() {
    validate_with_common(
        "bubbles.wgsl",
        include_str!("../src/render/shaders/bubbles.wgsl"),
    );
}

#[test]
fn automata_shader_is_valid_wgsl() {
    validate_with_common(
        "automata.wgsl",
        include_str!("../src/render/shaders/automata.wgsl"),
    );
}

#[test]
fn neurons_shader_is_valid_wgsl() {
    validate_with_common(
        "neurons.wgsl",
        include_str!("../src/render/shaders/neurons.wgsl"),
    );
}

#[test]
fn clockwork_shader_is_valid_wgsl() {
    validate_with_common(
        "clockwork.wgsl",
        include_str!("../src/render/shaders/clockwork.wgsl"),
    );
}

#[test]
fn beacons_shader_is_valid_wgsl() {
    validate_with_common(
        "beacons.wgsl",
        include_str!("../src/render/shaders/beacons.wgsl"),
    );
}
