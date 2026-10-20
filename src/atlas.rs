use std::collections::HashMap;
use crate::png;
use crate::zipfile::ZipFile;

#[derive(Debug)]
pub struct Atlas {
    pub size: u32,
    names: HashMap<String, (u32, u32)>,
    pixels: Vec<u8>,
}

impl Atlas {
    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn uv_of(&self, name: &str) -> Option<(u32, u32)> {
        self.names.get(name).copied()
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Debug)]
pub enum Error {
    Zip(crate::zipfile::Error),
    Png(crate::png::Error),
    Missing(String),
    WrongSize(String),
}

pub fn build(jar: &str, block_texture_names: &[&str]) -> Result<Atlas, Error> {
    let (t, size) = grid_size(block_texture_names.len());
    let zf = ZipFile::from_path(jar).map_err(Error::Zip)?;
    let mut names: HashMap<String, (u32, u32)> = HashMap::new();
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for (i, name) in block_texture_names.iter().enumerate() {
        let path = format!("assets/minecraft/textures/block/{}.png", name);
        if !zf.contains(&path) {
            return Err(Error::Missing(name.to_string()));
        }
        let data = zf.read_entry(&path).map_err(Error::Zip)?;
        let img = png::decode(&data).map_err(Error::Png)?;
        if img.width != 16 || img.height == 0 || img.height % 16 != 0 {
            return Err(Error::WrongSize(name.to_string()));
        }
        let col = (i % t as usize) as u32;
        let row = (i / t as usize) as u32;
        names.insert(format!("minecraft:block/{}", name), (col, row));
        for y in 0..16usize {
            let dst = ((row * 16 + y as u32) as usize) * size as usize * 4
                + (col * 16) as usize * 4;
            pixels[dst..dst + 64].copy_from_slice(&img.pixels[y * 64..(y + 1) * 64]);
        }
    }
    Ok(Atlas { size, names, pixels })
}

fn grid_size(n: usize) -> (u32, u32) {
    if n == 0 {
        return (0, 0);
    }
    let mut t = (n as f64).sqrt() as u32;
    if (t as u64) * (t as u64) < n as u64 {
        t += 1;
    }
    let mut v = 16u64 * t as u64;
    while v & (v - 1) != 0 {
        v += 1;
    }
    (t, v.min(u32::MAX as u64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAR: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";

    fn first_names(n: usize) -> Vec<String> {
        let zf = ZipFile::from_path(JAR).expect("jar opens");
        zf.entry_names()
            .into_iter()
            .filter(|e| {
                e.starts_with("assets/minecraft/textures/block/") && e.ends_with(".png")
            })
            .filter_map(|e| {
                let name = &e["assets/minecraft/textures/block/".len()..e.len() - 4];
                let data = zf.read_entry(e).expect("entry reads");
                if png::decode(&data).is_err() {
                    return None;
                }
                Some(name.to_string())
            })
            .take(n)
            .collect()
    }

    #[test]
    fn layout_for_50_names() {
        let names = first_names(50);
        assert_eq!(names.len(), 50);
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let a = build(JAR, &refs).expect("50-name build");
        let s = a.size();
        assert_eq!(s, 128);
        assert!(s & (s - 1) == 0);
        assert_eq!(s % 16, 0);
        assert_eq!(a.pixels().len(), (s * s * 4) as usize);
        for (i, name) in names.iter().enumerate() {
            let full = format!("minecraft:block/{}", name);
            assert_eq!(a.uv_of(&full), Some(((i % 8) as u32, (i / 8) as u32)));
        }
        assert_eq!(a.uv_of("acacia_door_bottom"), None);
        assert_eq!(a.uv_of("minecraft:block/no_such_texture"), None);
    }

    #[test]
    fn tile_pixels_match_source_png() {
        let zf = ZipFile::from_path(JAR).expect("jar opens");
        let data = zf
            .read_entry("assets/minecraft/textures/block/stone.png")
            .expect("stone entry");
        let img = png::decode(&data).expect("stone decodes");
        let a = build(JAR, &["stone"]).expect("single build");
        assert_eq!(a.size(), 16);
        let (col, row) = a.uv_of("minecraft:block/stone").expect("uv");
        let s = a.size() as usize;
        for y in 0..16usize {
            let off =
                ((row * 16 + y as u32) as usize) * s * 4 + (col * 16) as usize * 4;
            assert_eq!(&a.pixels()[off..off + 64], &img.pixels[y * 64..(y + 1) * 64]);
        }
    }

    #[test]
    fn unused_tiles_are_transparent() {
        let names = first_names(50);
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let a = build(JAR, &refs).expect("50-name build");
        let s = a.size() as usize;
        let tail = s - 16;
        for y in 0..16usize {
            let off = (tail + y) * s * 4 + tail * 4;
            assert_eq!(&a.pixels()[off..off + 64], &[0u8; 64]);
        }
        for y in 0..16usize {
            let off = (6 * 16 + y) * s * 4 + 7 * 16 * 4;
            assert_eq!(&a.pixels()[off..off + 64], &[0u8; 64]);
        }
        let corner = (s - 1) * s * 4 + (s - 1) * 4;
        assert_eq!(&a.pixels()[corner..corner + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn missing_texture_errors() {
        let err = build(JAR, &["definitely_not_a_texture"])
            .expect_err("must fail");
        match err {
            Error::Missing(n) => assert_eq!(n, "definitely_not_a_texture"),
            other => panic!("expected Missing, got {:?}", other),
        }
    }
}
