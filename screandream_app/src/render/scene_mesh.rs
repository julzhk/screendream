use std::path::{Component, Path};

pub const MAX_VERTICES: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub struct SceneMesh {
    pub vertices: Vec<[f32; 3]>,
    /// Triangle indices; polygon faces are fan-triangulated.
    pub indices: Vec<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum SceneMeshError {
    #[error("mesh must be loaded from the bundled assets directory")]
    OutsideAssets,
    #[error("PLY mesh is empty or malformed")]
    Invalid,
    #[error("PLY mesh exceeds the vertex limit")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl SceneMesh {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneMeshError> {
        let path = path.as_ref();
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
            || !path.starts_with("assets")
        {
            return Err(SceneMeshError::OutsideAssets);
        }
        let text = std::fs::read_to_string(path)?;
        let (_, body) = text
            .split_once("end_header\n")
            .ok_or(SceneMeshError::Invalid)?;
        let vertices: Vec<[f32; 3]> = body
            .lines()
            .take_while(|line| line.split_whitespace().count() == 3)
            .map(|line| {
                let values: Result<Vec<f32>, _> = line.split_whitespace().map(str::parse).collect();
                values.map(|values| [values[0], values[1], values[2]])
            })
            .collect::<Result<_, _>>()
            .map_err(|_| SceneMeshError::Invalid)?;
        if vertices.is_empty() {
            return Err(SceneMeshError::Invalid);
        }
        if vertices.len() > MAX_VERTICES {
            return Err(SceneMeshError::TooLarge);
        }
        let mut indices = Vec::new();
        for line in body.lines().skip(vertices.len()) {
            let values: Vec<u32> = line
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .map_err(|_| SceneMeshError::Invalid)?;
            let Some((&count, face)) = values.split_first() else {
                continue;
            };
            if count < 3
                || face.len() != count as usize
                || face.iter().any(|&index| index as usize >= vertices.len())
            {
                return Err(SceneMeshError::Invalid);
            }
            for i in 1..face.len() - 1 {
                indices.extend([face[0], face[i], face[i + 1]]);
            }
        }
        Ok(Self { vertices, indices })
    }
}
