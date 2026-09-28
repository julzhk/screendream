use screendream::render::scene_mesh::{SceneMesh, SceneMeshError};

#[test]
fn loads_the_bundled_ply_mesh() {
    let mesh = SceneMesh::load("assets/default-scene.ply").unwrap();
    assert_eq!(mesh.vertices.len(), 4);
}

#[test]
fn rejects_paths_outside_assets() {
    assert!(matches!(
        SceneMesh::load("../assets/default-scene.ply"),
        Err(SceneMeshError::OutsideAssets)
    ));
}
