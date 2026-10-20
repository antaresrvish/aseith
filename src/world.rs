use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use flate2::read::ZlibDecoder;
use std::io::Read;

use crate::atlas;
use crate::blockstates;
use crate::level;
use crate::mesh::{BlockClass, Face, VoxelView};
use crate::models;
use crate::nbt::reader;
use crate::nbt::value::Value;
use crate::region::{self, Region};
use crate::section;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Level(level::Error),
    Nbt(crate::nbt::error::Error),
    Blockstates(blockstates::Error),
    Models(models::Error),
    Region(region::Error),
    Section(section::Error),
    Atlas(atlas::Error),
    Missing(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Level(e) => write!(f, "level: {e}"),
            Error::Nbt(e) => write!(f, "nbt: {e}"),
            Error::Blockstates(e) => write!(f, "blockstates: {e}"),
            Error::Models(e) => write!(f, "models: {e}"),
            Error::Region(e) => write!(f, "region: {e}"),
            Error::Section(e) => write!(f, "section: {e}"),
            Error::Atlas(e) => write!(f, "atlas: {e:?}"),
            Error::Missing(name) => write!(f, "missing {name}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct World {
    pub level: level::LevelData,
    pub chunks: HashMap<(i32, i32), Chunk>,
}

struct StateTable {
    classes: Vec<BlockClass>,
    uvs: Vec<(f32, f32)>,
}

pub struct Chunk {
    x: i32,
    z: i32,
    height: u16,
    states: Vec<u32>,
    sky: Vec<u8>,
    block: Vec<u8>,
    table: Arc<StateTable>,
}

impl VoxelView for Chunk {
    fn dims(&self) -> (u8, u16, u8) {
        (16, self.height, 16)
    }

    fn state_at(&self, x: u8, y: u16, z: u8) -> u32 {
        *self.states.get(voxel_index(x, y, z)).unwrap_or(&0)
    }

    fn class_of(&self, state: u32) -> BlockClass {
        *self.table.classes.get(state as usize).unwrap_or(&BlockClass::Air)
    }

    fn light_at(&self, x: u8, y: u16, z: u8) -> (u8, u8) {
        let i = voxel_index(x, y, z);
        (*self.sky.get(i).unwrap_or(&0), *self.block.get(i).unwrap_or(&0))
    }

    fn uv_of(&self, state: u32, _face: Face) -> (f32, f32) {
        *self.table.uvs.get(state as usize).unwrap_or(&(0.0, 0.0))
    }
}

fn voxel_index(x: u8, y: u16, z: u8) -> usize {
    y as usize * 4096 + x as usize * 16 + z as usize
}

struct RawChunk {
    x: i32,
    z: i32,
    height: u16,
    states: Vec<u32>,
    sky: Vec<u8>,
    block: Vec<u8>,
}

fn section_palette(entry: &Value, registry: &blockstates::Registry) -> Result<Vec<u32>, Error> {
    let mut palette = Vec::new();
    if let Some(list) = entry
        .get("block_states")
        .and_then(|bs| bs.get("palette"))
        .and_then(Value::as_list)
    {
        for item in list {
            let name = item.get("Name").and_then(Value::as_str);
            palette.push(name.and_then(|name| registry.state_of(name)).unwrap_or(0));
        }
    }
    Ok(palette)
}

fn build_chunk(root: &Value, registry: &blockstates::Registry) -> Result<Option<RawChunk>, Error> {
    let x = root.get("xPos").and_then(Value::as_i32).ok_or(Error::Missing("xPos"))?;
    let z = root.get("zPos").and_then(Value::as_i32).ok_or(Error::Missing("zPos"))?;
    if root.get("status").and_then(Value::as_str) == Some("empty") {
        return Ok(None);
    }

    let mut decoded = Vec::new();
    if let Some(sections) = root.get("sections").and_then(Value::as_list) {
        for entry in sections {
            let sec = section::decode_section(entry).map_err(Error::Section)?;
            decoded.push((sec, section_palette(entry, registry)?));
        }
    }

    let height = decoded
        .iter()
        .filter(|(sec, _)| sec.y >= 0)
        .map(|(sec, _)| (sec.y as i32 + 1) * 16)
        .max()
        .unwrap_or(16) as u16;
    let size = height as usize * 4096;
    let mut states = vec![0u32; size];
    let mut sky = vec![0u8; size];
    let mut block = vec![0u8; size];

    for (sec, palette) in decoded {
        if sec.y < 0 {
            continue;
        }
        let base = sec.y as usize * 4096;
        for i in 0..4096usize {
            states[base + i] = palette.get(sec.states[i] as usize).copied().unwrap_or(0);
            sky[base + i] = sec.sky_light[i];
            block[base + i] = sec.block_light[i];
        }
    }

    Ok(Some(RawChunk { x, z, height, states, sky, block }))
}

fn state_parts<'a>(name: &'a str) -> (&'a str, Vec<(&'a str, &'a str)>) {
    let mut props = Vec::new();
    let base = match name.find('[') {
        Some(idx) => {
            for pair in name[idx + 1..].trim_end_matches(']').split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    props.push((k, v));
                }
            }
            &name[..idx]
        }
        None => name,
    };
    (base, props)
}

fn classify(name: &str) -> BlockClass {
    let (base, _) = state_parts(name);
    let n = base.to_ascii_lowercase();
    if n.contains("water") || n.contains("lava") {
        BlockClass::Liquid
    } else if n.contains("torch") || n.contains("leaves") {
        BlockClass::Cutout
    } else if n.contains("glass") {
        BlockClass::Translucent
    } else {
        BlockClass::Opaque
    }
}

fn state_texture<'m>(models: &'m models::Models, name: &str) -> Option<&'m str> {
    let (base, props) = state_parts(name);
    let model = models.for_blockstate(base, &props)?;
    if let Some(tex) = model.face_texture(models::Face::Up) {
        return Some(tex);
    }
    let faces = [
        models::Face::Down,
        models::Face::North,
        models::Face::South,
        models::Face::West,
        models::Face::East,
    ];
    faces.iter().find_map(|f| model.face_texture(*f))
}

fn decompress(zlib: &[u8]) -> Result<Vec<u8>, Error> {
    let mut dec = ZlibDecoder::new(zlib);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).map_err(Error::Io)?;
    Ok(out)
}

pub fn load_world(world_dir: &str, jar: &str) -> Result<World, Error> {
    let base = PathBuf::from(world_dir);
    let bytes = fs::read(base.join("level.dat")).map_err(Error::Io)?;
    let level = level::read_level_dat(&bytes).map_err(Error::Level)?;
    let registry = blockstates::load_registry(jar).map_err(Error::Blockstates)?;
    let models = models::load_models(jar).map_err(Error::Models)?;

    let mut raw = Vec::new();
    let region_dir = base.join("region");
    let entries = match fs::read_dir(&region_dir) {
        Ok(e) => e,
        Err(_) => return Ok(World { level, chunks: HashMap::new() }),
    };
    let mut regions: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| region::parse_region_name(name).is_some())
        .collect();
    regions.sort();
    for region_name in regions {
        let region = Region::read_region(&base.join("region").join(&region_name))
            .map_err(Error::Region)?;
        for cz in 0..32u8 {
            for cx in 0..32u8 {
                let Some(_) = region.slot(cx, cz) else { continue };
                let zlib = region.chunk_nbt(cx, cz).map_err(Error::Region)?;
                let bytes = decompress(&zlib)?;
                let root = reader::read(&bytes).map_err(Error::Nbt)?;
                if let Some(raw_chunk) = build_chunk(&root, &registry)? {
                    raw.push(raw_chunk);
                }
            }
        }
    }

    let ids: HashSet<u32> = raw
        .iter()
        .flat_map(|c| c.states.iter().copied())
        .filter(|id| *id != 0)
        .collect();
    let table = build_table(&ids, jar, &registry, &models)?;
    let mut chunks = HashMap::new();
    for c in raw {
        chunks.insert((c.x, c.z), Chunk {
            x: c.x,
            z: c.z,
            height: c.height,
            states: c.states,
            sky: c.sky,
            block: c.block,
            table: table.clone(),
        });
    }
    Ok(World { level, chunks })
}

fn build_table(
    ids: &HashSet<u32>,
    jar: &str,
    registry: &blockstates::Registry,
    models: &models::Models,
) -> Result<Arc<StateTable>, Error> {
    let total = registry.total_states() as usize;
    let mut classes = vec![BlockClass::Air; total];
    let mut uvs = vec![(0.0f32, 0.0f32); total];
    let mut seen: HashSet<&str> = HashSet::new();
    let mut names: Vec<&str> = Vec::new();
    for &id in ids {
        if let Some(name) = registry.name_of(id) {
            classes[id as usize] = classify(name);
            if let Some(tex) = state_texture(models, name) {
                let plain = tex.strip_prefix("minecraft:block/").unwrap_or(tex);
                if seen.insert(plain) {
                    names.push(plain);
                }
            }
        }
    }
    names.sort_unstable();
    let atlas = atlas::build(jar, &names).map_err(Error::Atlas)?;
    let size = atlas.size() as f32;
    for &id in ids {
        let name = match registry.name_of(id) {
            Some(name) => name,
            None => continue,
        };
        let tex = match state_texture(models, name) {
            Some(tex) => tex,
            None => continue,
        };
        if let Some((col, row)) = atlas.uv_of(tex) {
            uvs[id as usize] = (col as f32 * 16.0 / size, row as f32 * 16.0 / size);
        }
    }
    Ok(Arc::new(StateTable { classes, uvs }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/instances/aseith/minecraft/saves/anvil";
    const JAR: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";

    fn world() -> World {
        load_world(WORLD, JAR).expect("load_world")
    }

    fn non_air_chunk(w: &World) -> &Chunk {
        let mut keys: Vec<(i32, i32)> = w.chunks.keys().copied().collect();
        keys.sort();
        for k in keys {
            let c = &w.chunks[&k];
            if c.states.iter().any(|s| *s != 0) {
                return c;
            }
        }
        panic!("no non-air chunk");
    }

    fn lit_chunk(w: &World) -> &Chunk {
        let mut keys: Vec<(i32, i32)> = w.chunks.keys().copied().collect();
        keys.sort();
        for k in keys {
            let c = &w.chunks[&k];
            if !c.states.iter().any(|&s| s != 0) {
                continue;
            }
            if c.sky.iter().any(|&v| v != 0) || c.block.iter().any(|&v| v != 0) {
                return c;
            }
        }
        panic!("no lit chunk");
    }

    fn ids_of(w: &World) -> HashSet<u32> {
        w.chunks
            .values()
            .flat_map(|c| c.states.iter().copied())
            .filter(|id| *id != 0)
            .collect()
    }

    fn texture_names(ids: &HashSet<u32>, jar: &str) -> Vec<String> {
        let registry = blockstates::load_registry(jar).expect("registry");
        let models = models::load_models(jar).expect("models");
        let mut seen: HashSet<&str> = HashSet::new();
        let mut names: Vec<&str> = Vec::new();
        for &id in ids {
            let name = match registry.name_of(id) {
                Some(name) => name,
                None => continue,
            };
            let tex = match state_texture(&models, name) {
                Some(tex) => tex,
                None => continue,
            };
            let plain = tex.strip_prefix("minecraft:block/").unwrap_or(tex);
            if seen.insert(plain) {
                names.push(plain);
            }
        }
        names.sort_unstable();
        names.into_iter().map(str::to_owned).collect()
    }

    #[test]
    fn level_and_chunk_dims() {
        let w = world();
        assert_eq!(w.level.data_version, 3465);
        assert!(!w.chunks.is_empty());
        let c = non_air_chunk(&w);
        let (w0, h, d) = c.dims();
        assert_eq!((w0, d), (16, 16));
        assert!(h >= 16);
        assert_eq!(c.states.len(), h as usize * 4096);
    }

    #[test]
    fn mesh_has_geometry() {
        let w = world();
        let c = non_air_chunk(&w);
        let m = crate::mesh::mesh_chunk(c);
        let ok = [m.opaque, m.cutout, m.translucent, m.liquid]
            .iter()
            .any(|m| m.vertex_count() > 0 && !m.indices.is_empty());
        assert!(ok, "no mesh pass produced geometry");
    }

    #[test]
    fn voxel_state_and_light() {
        let w = world();
        let c = lit_chunk(&w);
        let i = c.states.iter().position(|s| *s != 0).expect("state");
        let y = (i / 4096) as u16;
        let x = ((i % 4096) / 16) as u8;
        let z = (i % 16) as u8;
        assert_ne!(c.state_at(x, y, z), 0);
        let (w0, h, d) = c.dims();
        let mut lit = 0;
        for y in 0..h {
            for x in 0..w0 {
                for z in 0..d {
                    let (sky, block) = c.light_at(x, y, z);
                    assert!(sky <= 15 && block <= 15);
                    if sky > 0 || block > 0 {
                        lit += 1;
                    }
                }
            }
        }
        assert!(lit > 0);
    }

    #[test]
    fn sky_light_present() {
        let w = world();
        let mut keys: Vec<(i32, i32)> = w.chunks.keys().copied().collect();
        keys.sort();
        for k in keys {
            let c = &w.chunks[&k];
            let (w0, h, d) = c.dims();
            for y in 0..h {
                for x in 0..w0 {
                    for z in 0..d {
                        if c.light_at(x, y, z).0 > 0 {
                            return;
                        }
                    }
                }
            }
        }
        panic!("no sky light in any chunk");
    }

    #[test]
    fn uv_matches_atlas_tiles() {
        let w = world();
        let ids = ids_of(&w);
        assert!(!ids.is_empty());
        let names = texture_names(&ids, JAR);
        assert!(!names.is_empty());
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let atlas = atlas::build(JAR, &refs).expect("atlas");
        let size = atlas.size() as f32;
        let registry = blockstates::load_registry(JAR).expect("registry");
        let models = models::load_models(JAR).expect("models");
        let c = non_air_chunk(&w);
        for &id in &ids {
            let name = match registry.name_of(id) {
                Some(name) => name,
                None => continue,
            };
            let tex = match state_texture(&models, name) {
                Some(tex) => tex,
                None => continue,
            };
            let (col, row) = match atlas.uv_of(tex) {
                Some(v) => v,
                None => continue,
            };
            let want = (col as f32 * 16.0 / size, row as f32 * 16.0 / size);
            assert_eq!(c.uv_of(id, Face::PosY), want);
        }
    }
}
