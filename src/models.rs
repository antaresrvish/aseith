use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::models_bake::{bake_model, cullface, normalize_ref, pick_variant_key, variant_model, ChainLink, RawModel};
use crate::zipfile::ZipFile;

#[derive(Debug)]
pub enum Error {
    Zip(crate::zipfile::Error),
    Json(serde_json::Error),
    InvalidFace(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Json(e) => write!(f, "json error: {e}"),
            Error::InvalidFace(s) => write!(f, "invalid face: {s}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face {
    pub(crate) fn parse(name: &str) -> Option<Face> {
        match name {
            "down" => Some(Face::Down),
            "up" => Some(Face::Up),
            "north" => Some(Face::North),
            "south" => Some(Face::South),
            "west" => Some(Face::West),
            "east" => Some(Face::East),
            "bottom" => Some(Face::Down),
            "top" => Some(Face::Up),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cullface {
    Always,
    Face(Face),
}

#[derive(Debug, Clone)]
pub struct FaceData {
    pub texture: String,
    pub uv: [f32; 4],
    pub cull_face: Option<Cullface>,
    pub shade: Option<bool>,
    pub tintindex: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub shade: Option<bool>,
    pub faces: Vec<(Face, FaceData)>,
}

pub struct Model {
    parent: Option<String>,
    elements: Vec<Element>,
    textures: HashMap<String, String>,
    pub(crate) is_cross: bool,
    pub(crate) has_elements: bool,
    pub(crate) face_textures: [Option<String>; 6],
}

impl Model {
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn is_cross(&self) -> bool {
        self.is_cross
    }

    pub fn face_texture(&self, face: Face) -> Option<&str> {
        self.face_textures[face as usize].as_deref()
    }
}

pub struct Models {
    jar: ZipFile,
    models: HashMap<String, Model>,
}

pub fn load_models<P: AsRef<Path>>(jar_path: P) -> Result<Models, Error> {
    let jar = ZipFile::from_path(jar_path).map_err(Error::Zip)?;
    let mut models: HashMap<String, Model> = HashMap::new();
    for name in jar.entry_names() {
        let Some(file) = name
            .strip_prefix("assets/minecraft/models/block/")
            .and_then(|f| f.strip_suffix(".json"))
        else {
            continue;
        };
        if file.contains('/') {
            continue;
        }
        let bytes = jar.read_entry(name).map_err(Error::Zip)?;
        let raw: RawModel = serde_json::from_slice(&bytes).map_err(Error::Json)?;
        let ref_ = format!("minecraft:block/{file}");
        let parent = raw.parent.as_deref().map(normalize_ref);
        let elements = raw
            .elements
            .iter()
            .map(|e| {
                let mut faces = Vec::new();
                for (k, f) in &e.faces {
                    let face = Face::parse(k).ok_or(Error::InvalidFace(k.clone()))?;
                    faces.push((face, FaceData {
                        texture: f.texture.clone(),
                        uv: f.uv.unwrap_or([0.0; 4]),
                        cull_face: cullface(&f.cullface)?,
                        shade: f.shade,
                        tintindex: f.tintindex,
                    }));
                }
                faces.sort_unstable_by_key(|(f, _)| *f as usize);
                Ok(Element {
                    from: e.from,
                    to: e.to,
                    shade: e.shade,
                    faces,
                })
            })
            .collect::<Result<Vec<Element>, Error>>()?;
        models.insert(ref_, Model {
            parent,
            elements,
            textures: raw.textures,
            is_cross: false,
            has_elements: false,
            face_textures: std::array::from_fn(|_| None),
        });
    }
    let mut models = Models { jar, models };
    let names: Vec<String> = models.models.keys().cloned().collect();
    for name in names {
        let chain = models.chain(&name);
        if let Some(model) = models.models.get_mut(&name) {
            bake_model(model, &chain);
        }
    }
    Ok(models)
}

impl Models {
    fn chain(&self, name: &str) -> Vec<ChainLink> {
        let mut links = Vec::new();
        let mut seen = Vec::new();
        let mut cur = name;
        while let Some(model) = self.models.get(cur) {
            if seen.iter().any(|s| *s == cur) {
                break;
            }
            seen.push(cur);
            links.push(ChainLink {
                parent: model.parent.clone(),
                textures: model.textures.clone(),
                elements: model.elements.clone(),
            });
            match model.parent.as_deref() {
                Some(parent) => cur = parent,
                None => break,
            }
        }
        links
    }

    pub fn for_blockstate(&self, name: &str, props: &[(&str, &str)]) -> Option<&Model> {
        let file = name
            .strip_prefix("minecraft:")
            .unwrap_or(name)
            .rsplit('/')
            .next()
            .unwrap_or(name);
        let path = format!("assets/minecraft/blockstates/{file}.json");
        let bytes = self.jar.read_entry(&path).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let variants = value.get("variants")?.as_object()?;
        let key = pick_variant_key(variants, props)?;
        let value = &variants[key];
        let model = variant_model(value).or_else(|| {
            value
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.get("model"))
                .and_then(|m| m.as_str())
        })?;
        self.models
            .get(normalize_ref(model).as_str())
            .filter(|m| m.has_elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAR: &str =
        "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";

    fn models() -> Models {
        load_models(JAR).expect("load models from jar")
    }

    #[test]
    fn stone_resolves_via_parent_chain() {
        let m = models();
        let model = m.for_blockstate("minecraft:stone", &[]).expect("stone");
        assert!(model.elements().is_empty());
        assert!(!model.is_cross());
        for face in [Face::Down, Face::Up, Face::North, Face::South, Face::West, Face::East] {
            assert_eq!(model.face_texture(face), Some("minecraft:block/stone"));
        }
    }

    #[test]
    fn grass_block_top_side_bottom() {
        let m = models();
        let model = m.for_blockstate("minecraft:grass_block", &[]).expect("grass_block");
        assert_eq!(model.face_texture(Face::Up), Some("minecraft:block/grass_block_top"));
        assert_eq!(model.face_texture(Face::Down), Some("minecraft:block/dirt"));
        for face in [Face::North, Face::South, Face::West, Face::East] {
            assert_eq!(model.face_texture(face), Some("minecraft:block/grass_block_side"));
        }
    }

    #[test]
    fn air_is_none() {
        let m = models();
        assert!(m.for_blockstate("minecraft:air", &[]).is_none());
    }

    #[test]
    fn dandelion_is_cross() {
        let m = models();
        let model = m.for_blockstate("minecraft:dandelion", &[]).expect("dandelion");
        assert!(model.is_cross());
        assert_eq!(model.face_texture(Face::North), Some("minecraft:block/dandelion"));
        assert_eq!(model.face_texture(Face::South), Some("minecraft:block/dandelion"));
    }

    #[test]
    fn red_sandstone_cube_bottom_top() {
        let m = models();
        let model = m.for_blockstate("minecraft:red_sandstone", &[]).expect("red_sandstone");
        assert_eq!(model.face_texture(Face::Up), Some("minecraft:block/red_sandstone_top"));
        assert_eq!(model.face_texture(Face::Down), Some("minecraft:block/red_sandstone_bottom"));
        for face in [Face::North, Face::South, Face::West, Face::East] {
            assert_eq!(model.face_texture(face), Some("minecraft:block/red_sandstone"));
        }
    }

    #[test]
    fn unknown_blockstate_is_none() {
        let m = models();
        assert!(m.for_blockstate("minecraft:does_not_exist", &[]).is_none());
    }
}

