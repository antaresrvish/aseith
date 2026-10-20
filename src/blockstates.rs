use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::Path;

use flate2::read::ZlibDecoder;

use crate::blockstates_data::TABLE;
use crate::zipfile::ZipFile;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(crate::zipfile::Error),
    InvalidTable,
    MissingBlockstate(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::InvalidTable => write!(f, "invalid embedded table"),
            Error::MissingBlockstate(name) => write!(f, "missing blockstate {name} in jar"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Registry {
    pool: Vec<u8>,
    entries: Vec<(u32, usize, usize, u32)>,
    by_name: HashMap<String, u32>,
}

impl Registry {
    pub fn name_of(&self, id: u32) -> Option<&str> {
        let mut lo = 0usize;
        let mut hi = self.entries.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.entries[mid].0 <= id {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let (min, start, len, count) = self.entries[lo - 1];
        if id >= min + count {
            return None;
        }
        let name = std::str::from_utf8(&self.pool[start..start + len]).ok()?;
        Some(name)
    }

    pub fn state_of(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn total_states(&self) -> u32 {
        match self.entries.last() {
            Some(&(min, _, _, count)) => min + count,
            None => 0,
        }
    }

    pub fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|(_, start, len, _)| {
                std::str::from_utf8(&self.pool[*start..*start + *len]).ok()
            })
            .collect()
    }
}

fn base64_value(c: char) -> Option<u32> {
    match c {
        'A'..='Z' => Some(c as u32 - 'A' as u32),
        'a'..='z' => Some(26 + (c as u32 - 'a' as u32)),
        '0'..='9' => Some(52 + (c as u32 - '0' as u32)),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in s.chars() {
        if c == ' ' || c == '\n' || c == '\r' {
            continue;
        }
        acc = (acc << 6) | base64_value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1u32 << bits) - 1;
        }
    }
    Some(out)
}

fn decode_table() -> Result<Vec<(String, u32, u32)>, Error> {
    let bytes = base64_decode(TABLE).ok_or(Error::InvalidTable)?;
    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut text = Vec::new();
    decoder.read_to_end(&mut text).map_err(|_| Error::InvalidTable)?;
    let text = std::str::from_utf8(&text).map_err(|_| Error::InvalidTable)?;
    let mut rows = Vec::new();
    let mut id = 0u32;
    for line in text.lines() {
        let (name, count) = line.split_once(' ').ok_or(Error::InvalidTable)?;
        let count: u32 = count.parse().map_err(|_| Error::InvalidTable)?;
        rows.push((name.to_string(), id, count));
        id += count;
    }
    Ok(rows)
}

pub fn load_registry<P: AsRef<Path>>(jar_path: P) -> Result<Registry, Error> {
    let rows = decode_table()?;
    let zip = ZipFile::from_path(jar_path).map_err(Error::Zip)?;
    let mut pool = Vec::new();
    let mut entries = Vec::new();
    let mut by_name = HashMap::new();
    for (name, min, count) in &rows {
        let short = name.strip_prefix("minecraft:").unwrap_or(name);
        if !zip.contains(&format!("assets/minecraft/blockstates/{short}.json")) {
            return Err(Error::MissingBlockstate(name.clone()));
        }
        let start = pool.len();
        pool.extend_from_slice(name.as_bytes());
        entries.push((*min, start, name.len(), *count));
        by_name.insert(name.clone(), *min);
    }
    Ok(Registry { pool, entries, by_name })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAR: &str =
        "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";

    fn registry() -> Registry {
        load_registry(JAR).expect("load registry from jar")
    }

    #[test]
    fn anchors_match_vanilla_registration_order() {
        let reg = registry();
        assert_eq!(reg.name_of(0), Some("minecraft:air"));
        assert_eq!(reg.name_of(1), Some("minecraft:stone"));
        assert_eq!(reg.state_of("minecraft:oak_sapling"), Some(25));
        assert_eq!(reg.name_of(25), Some("minecraft:oak_sapling"));
        assert_eq!(reg.name_of(26), Some("minecraft:oak_sapling"));
        assert_eq!(reg.total_states(), 24135);
        assert!(reg.name_of(24134).is_some());
        assert_eq!(reg.name_of(24135), None);
        assert_eq!(reg.state_of("minecraft:does_not_exist"), None);
    }

    #[test]
    fn name_and_state_are_inverses() {
        let reg = registry();
        for name in reg.names() {
            let id = reg.state_of(name).expect("state_of table name");
            assert_eq!(reg.name_of(id), Some(name));
        }
    }

    #[test]
    fn jar_blockstate_superset_of_table() {
        let reg = registry();
        let zip = ZipFile::from_path(JAR).expect("open jar");
        let jar_names: Vec<&str> = zip
            .entry_names()
            .iter()
            .filter(|n| n.starts_with("assets/minecraft/blockstates/") && n.ends_with(".json"))
            .map(|n| &n["assets/minecraft/blockstates/".len()..n.len() - 5])
            .collect();
        let extra: Vec<&str> = jar_names
            .iter()
            .filter(|n| reg.state_of(&format!("minecraft:{n}")).is_none())
            .copied()
            .collect();
        println!("jar-only blockstates: {extra:?}");
        assert_eq!(extra, vec!["glow_item_frame", "item_frame"]);
    }
}
