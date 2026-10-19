use std::fmt;
use std::io;

use crate::nbt::error::Error as NbtError;
use crate::nbt::reader;
use crate::nbt::value::Value;

#[derive(Debug, PartialEq)]
pub struct LevelData {
    pub data_version: i32,
    pub version: Option<i32>,
    pub random_seed: i64,
    pub spawn: [i32; 3],
    pub name: String,
    pub last_played: Option<i64>,
}

#[derive(Debug)]
pub enum Error {
    Nbt(NbtError),
    Io(io::Error),
    Missing(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Nbt(e) => write!(f, "nbt: {e}"),
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Missing(name) => write!(f, "missing required field {name}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn read_level_dat(bytes: &[u8]) -> Result<LevelData, Error> {
    let root = reader::read_gzip(bytes).map_err(Error::Nbt)?;
    let data = root.get("Data").ok_or(Error::Missing("Data"))?;
    let mut random_seed = 0i64;
    if let Some(seed) = data.get("RandomSeed").and_then(Value::as_i64) {
        random_seed = seed;
    }
    Ok(LevelData {
        data_version: int(data, "DataVersion")?,
        version: data.get("version").and_then(Value::as_i32),
        random_seed,
        spawn: [
            spawn_int(data, "spawnX", "SpawnX")?,
            spawn_int(data, "spawnY", "SpawnY")?,
            spawn_int(data, "spawnZ", "SpawnZ")?,
        ],
        name: data
            .get("LevelName")
            .and_then(Value::as_str)
            .ok_or(Error::Missing("LevelName"))?
            .to_owned(),
        last_played: data.get("LastPlayed").and_then(Value::as_i64),
    })
}

fn int(data: &Value, name: &'static str) -> Result<i32, Error> {
    data.get(name).and_then(Value::as_i32).ok_or(Error::Missing(name))
}

fn spawn_int(data: &Value, a: &'static str, b: &'static str) -> Result<i32, Error> {
    match data.get(a) {
        Some(_) => int(data, a),
        None => int(data, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    const WORLD_LEVEL_DAT: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/instances/aseith/minecraft/saves/anvil/level.dat";

    fn be16(v: u16) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    fn be32(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    fn be64(v: i64) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    fn name(s: &str) -> Vec<u8> {
        let mut out = be16(s.len() as u16);
        out.extend(s.bytes());
        out
    }

    fn tag_int(n: &str, v: i32) -> Vec<u8> {
        let mut out = vec![3u8];
        out.extend(name(n));
        out.extend(be32(v));
        out
    }

    fn tag_long(n: &str, v: i64) -> Vec<u8> {
        let mut out = vec![4u8];
        out.extend(name(n));
        out.extend(be64(v));
        out
    }

    fn tag_str(n: &str, s: &str) -> Vec<u8> {
        let mut out = vec![8u8];
        out.extend(name(n));
        out.extend(be16(s.len() as u16));
        out.extend(s.bytes());
        out
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(data)
            .expect("gzip write succeeds");
        encoder.finish().expect("gzip finish succeeds")
    }

    fn level_dat(data_fields: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        for field in data_fields {
            data.extend(field);
        }
        data.push(0);
        let mut root = vec![10u8];
        root.extend(be16(0));
        root.push(10);
        root.extend(name("Data"));
        root.extend(data);
        root.push(0);
        gzip(&root)
    }

    fn full_fields() -> Vec<Vec<u8>> {
        vec![
            tag_int("DataVersion", 3465),
            tag_int("version", 19133),
            tag_long("RandomSeed", -4_655_615_948_655_692_221),
            tag_int("SpawnX", 117),
            tag_int("SpawnY", 64),
            tag_int("SpawnZ", -256),
            tag_str("LevelName", "anvil"),
            tag_long("LastPlayed", 1_760_000_000_000),
        ]
    }

    #[test]
    fn parses_synthetic_level_dat() {
        let data = read_level_dat(&level_dat(&full_fields())).expect("synthetic level.dat parses");
        assert_eq!(data.data_version, 3465);
        assert_eq!(data.version, Some(19133));
        assert_eq!(data.random_seed, -4_655_615_948_655_692_221);
        assert_eq!(data.spawn, [117, 64, -256]);
        assert_eq!(data.name, "anvil");
        assert_eq!(data.last_played, Some(1_760_000_000_000));
    }

    #[test]
    fn optional_fields_default_to_none() {
        let fields = vec![
            tag_int("DataVersion", 3193),
            tag_long("RandomSeed", 42),
            tag_int("spawnX", 0),
            tag_int("spawnY", 4),
            tag_int("spawnZ", 8),
            tag_str("LevelName", "anvil"),
        ];
        let data = read_level_dat(&level_dat(&fields)).expect("synthetic level.dat parses");
        assert_eq!(data.version, None);
        assert_eq!(data.last_played, None);
        assert_eq!(data.data_version, 3193);
    }

    #[test]
    fn missing_required_field_is_err() {
        let fields = vec![
            tag_int("spawnX", 117),
            tag_int("spawnY", 64),
            tag_int("spawnZ", -256),
            tag_str("LevelName", "anvil"),
        ];
        let err = read_level_dat(&level_dat(&fields)).expect_err("missing DataVersion is err");
        assert!(matches!(err, Error::Missing("DataVersion")), "{err:?}");
    }

    #[test]
    fn rejects_non_gzip_bytes() {
        let err = read_level_dat(b"not gzip at all").expect_err("non-gzip bytes are err");
        assert!(matches!(err, Error::Nbt(NbtError::InvalidGzip)), "{err:?}");
    }

    #[test]
    fn reads_real_world_level_dat() {
        let bytes = std::fs::read(WORLD_LEVEL_DAT).expect("real level.dat is present");
        let data = read_level_dat(&bytes).expect("real level.dat parses");
        assert_eq!(data.data_version, 3465);
        assert_eq!(data.version, Some(19133));
        assert_eq!(data.random_seed, 0);
        assert_eq!(data.spawn, [-144, 56, 0]);
        assert_eq!(data.name, "anvil");
        assert_eq!(data.last_played, Some(1_792_414_091_664));
    }
}
