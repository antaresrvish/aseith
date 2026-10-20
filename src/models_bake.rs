use std::collections::HashMap;

use serde::Deserialize;

use crate::models::{Cullface, Element, Error, Face, Model};

#[derive(Deserialize)]
pub(crate) struct RawModel {
    pub(crate) parent: Option<String>,
    #[serde(default)]
    pub(crate) textures: HashMap<String, String>,
    #[serde(default)]
    pub(crate) elements: Vec<RawElement>,
}

#[derive(Deserialize)]
pub(crate) struct RawElement {
    pub(crate) from: [f32; 3],
    pub(crate) to: [f32; 3],
    pub(crate) shade: Option<bool>,
    #[serde(default)]
    pub(crate) faces: HashMap<String, RawFace>,
}

#[derive(Deserialize)]
pub(crate) struct RawFace {
    pub(crate) texture: String,
    pub(crate) uv: Option<[f32; 4]>,
    pub(crate) cullface: Option<serde_json::Value>,
    pub(crate) shade: Option<bool>,
    pub(crate) tintindex: Option<i32>,
}

pub(crate) fn cullface(raw: &Option<serde_json::Value>) -> Result<Option<Cullface>, Error> {
    let Some(v) = raw else {
        return Ok(None);
    };
    if v.as_bool() == Some(true) {
        return Ok(Some(Cullface::Always));
    }
    if let Some(s) = v.as_str() {
        return Ok(Some(Cullface::Face(Face::parse(s).ok_or(Error::InvalidFace(s.to_string()))?)));
    }
    Err(Error::InvalidFace(v.to_string()))
}

fn canonical(ref_: &str) -> Option<String> {
    let mut s = ref_.trim();
    if let Some(r) = s.strip_prefix("minecraft:") {
        s = r;
    }
    let s = s.strip_suffix(".png").unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    Some(format!("minecraft:{s}"))
}

pub(crate) fn normalize_ref(ref_: &str) -> String {
    if ref_.starts_with("minecraft:") {
        ref_.to_string()
    } else {
        format!("minecraft:{ref_}")
    }
}

pub(crate) fn variant_model(value: &serde_json::Value) -> Option<&str> {
    value.get("model").and_then(|m| m.as_str())
}

pub(crate) fn pick_variant_key<'a>(
    variants: &'a serde_json::Map<String, serde_json::Value>,
    props: &[(&str, &str)],
) -> Option<&'a str> {
    let pairs: Vec<(&str, &str)> = props.iter().copied().collect();
    let mut best: Option<(usize, &str)> = None;
    for (key, _) in variants {
        let key_pairs = parse_key(key);
        let score = if props.is_empty() {
            if key_pairs.is_empty() {
                Some(0)
            } else {
                Some(defaultness(&key_pairs))
            }
        } else if key_pairs.len() == pairs.len() && pairs.iter().all(|(k, v)| key_pairs.iter().any(|(k2, v2)| k == k2 && v == v2)) {
            Some(0)
        } else {
            None
        };
        let Some(score) = score else { continue };
        if best.map(|(s, _)| score < s).unwrap_or(true) {
            best = Some((score, key.as_str()));
        }
    }
    best.map(|(_, k)| k)
}

fn parse_key(key: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    for part in key.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            out.push((k, v));
        }
    }
    out
}

fn defaultness(pairs: &[(&str, &str)]) -> usize {
    pairs
        .iter()
        .filter(|(_, v)| *v != "false" && *v != "north")
        .count()
}

pub(crate) struct ChainLink {
    pub(crate) parent: Option<String>,
    pub(crate) textures: HashMap<String, String>,
    pub(crate) elements: Vec<Element>,
}

pub(crate) fn bake_model(model: &mut Model, chain: &[ChainLink]) {
    let mut textures: HashMap<String, String> = HashMap::new();
    for m in chain.iter().rev() {
        for (k, v) in &m.textures {
            textures.insert(k.clone(), v.clone());
        }
    }
    model.is_cross = chain
        .iter()
        .any(|m| m.parent.as_deref().map(|p| p.ends_with("/cross")).unwrap_or(false));
    let Some(elements) = chain.iter().find(|m| !m.elements.is_empty()).map(|m| &m.elements) else {
        return;
    };
    model.has_elements = true;
    for face in [Face::Down, Face::Up, Face::North, Face::South, Face::West, Face::East] {
        let Some(fd) = elements
            .iter()
            .find_map(|e| e.faces.iter().find(|(f, _)| *f == face))
            .map(|(_, fd)| fd)
        else {
            continue;
        };
        let idx = match face {
            Face::Down => 0,
            Face::Up => 1,
            Face::North => 2,
            Face::South => 3,
            Face::West => 4,
            Face::East => 5,
        };
        let resolved = resolve_texture(&fd.texture, &textures);
        model.face_textures[idx] = resolved;
    }
}

fn resolve_texture(ref_: &str, textures: &HashMap<String, String>) -> Option<String> {
    let mut cur = ref_.to_string();
    let mut seen: Vec<String> = Vec::new();
    loop {
        let Some(v) = cur.strip_prefix('#') else {
            break;
        };
        if seen.iter().any(|s| s == v) {
            return None;
        }
        seen.push(v.to_string());
        cur = textures.get(v)?.clone();
    }
    canonical(&cur)
}
