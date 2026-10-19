use crate::nbt::value::Value;

#[derive(Debug, PartialEq)]
pub enum Error {
    Missing,
    Invalid,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing => write!(f, "missing section field"),
            Error::Invalid => write!(f, "invalid section data"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub y: i8,
    pub states: [u32; 4096],
    pub block_light: [u8; 4096],
    pub sky_light: [u8; 4096],
    pub biomes: Option<[u16; 64]>,
}

const VOXELS: usize = 4096;
const BIOME_SLOTS: usize = 64;

fn field<'a>(fields: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn bits_per_voxel(data_len: usize, count: usize) -> Option<u32> {
    let total = data_len as u64 * 64;
    let bits = total / count as u64;
    (bits > 0 && bits <= 32 && total == bits * count as u64).then_some(bits as u32)
}

fn unpack_bits(data: &[i64], bits: u32, out: &mut [u32]) -> Result<(), Error> {
    let mask = (1u64 << bits) - 1;
    for (i, slot) in out.iter_mut().enumerate() {
        let bit = i as u64 * bits as u64;
        let long = data.get((bit / 64) as usize).ok_or(Error::Invalid)?;
        *slot = ((*long as u64) >> (bit % 64) & mask) as u32;
    }
    Ok(())
}

fn decode_light(light: Option<&Value>) -> [u8; VOXELS] {
    let mut out = [0u8; VOXELS];
    if let Some(bytes) = light.and_then(Value::as_bytes) {
        for (slot, b) in out.iter_mut().zip(bytes) {
            *slot = *b as u8;
        }
    }
    out
}

fn decode_states(bs: &Value) -> Result<[u32; VOXELS], Error> {
    let palette = match bs.get("palette").and_then(Value::as_list) {
        Some(palette) if !palette.is_empty() => palette,
        _ => return Err(Error::Invalid),
    };
    let data = match bs.get("data").and_then(Value::as_long_array) {
        Some(data) => data,
        None => return Ok([0u32; VOXELS]),
    };
    let max = palette.len() as u32;
    for bits in (1u32..=8).rev() {
        let need = (VOXELS as u64 * bits as u64 + 63) / 64;
        if (data.len() as u64) < need {
            continue;
        }
        let mut states = [0u32; VOXELS];
        unpack_bits(data, bits, &mut states)?;
        if states.iter().all(|&s| s < max) {
            return Ok(states);
        }
    }
    Err(Error::Invalid)
}

fn decode_biomes(biomes: &Value) -> Result<Option<[u16; BIOME_SLOTS]>, Error> {
    let fields = match biomes.as_compound() {
        Some(fields) => fields,
        None => return Ok(None),
    };
    let palette = match field(fields, "palette").and_then(Value::as_list) {
        Some(palette) => palette,
        None => return Ok(None),
    };
    let data = match field(fields, "data").and_then(Value::as_long_array) {
        Some(data) => data,
        None => return Ok(None),
    };
    let bits = bits_per_voxel(data.len(), BIOME_SLOTS).ok_or(Error::Invalid)?;
    let mut biomes = [0u32; BIOME_SLOTS];
    unpack_bits(data, bits, &mut biomes)?;
    let max = palette.len() as u32;
    for b in biomes.iter() {
        if *b >= max {
            return Err(Error::Invalid);
        }
    }
    Ok(Some(biomes.map(|b| b as u16)))
}

pub fn decode_section(entry: &Value) -> Result<Section, Error> {
    let y = match entry.get("Y") {
        Some(Value::Byte(y)) => *y,
        _ => return Err(Error::Missing),
    };
    let block_states = entry.get("block_states").ok_or(Error::Missing)?;
    let states = decode_states(block_states)?;
    let block_light = decode_light(entry.get("BlockLight"));
    let sky_light = decode_light(entry.get("SkyLight"));
    let biomes = match entry.get("biomes") {
        Some(biomes) => decode_biomes(biomes)?,
        None => None,
    };
    Ok(Section {
        y,
        states,
        block_light,
        sky_light,
        biomes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::reader;
    use crate::region::Region;
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    const WORLD_REGION_DIR: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/instances/aseith/minecraft/saves/anvil/region/";

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

    fn tag_byte(n: &str, v: i8) -> Vec<u8> {
        let mut out = vec![1u8];
        out.extend(name(n));
        out.push(v as u8);
        out
    }

    fn tag_byte_array(n: &str, v: &[i8]) -> Vec<u8> {
        let mut out = vec![7u8];
        out.extend(name(n));
        out.extend(be32(v.len() as i32));
        out.extend(v.iter().map(|b| *b as u8));
        out
    }

    fn tag_long_array(n: &str, v: &[i64]) -> Vec<u8> {
        let mut out = vec![12u8];
        out.extend(name(n));
        out.extend(be32(v.len() as i32));
        for x in v {
            out.extend(be64(*x));
        }
        out
    }

    fn tag_compound(n: &str, fields: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![10u8];
        out.extend(name(n));
        for f in fields {
            out.extend(f);
        }
        out.push(0);
        out
    }

    fn tag_list(n: &str, elem: u8, items: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![9u8];
        out.extend(name(n));
        out.push(elem);
        out.extend(be32(items.len() as i32));
        for i in items {
            out.extend(i);
        }
        out
    }

    fn block_states(palette: &[&str], data: &[i64]) -> Vec<u8> {
        let entries: Vec<Vec<u8>> = palette
            .iter()
            .map(|p| bare_compound(&[str_tag("Name", p)]))
            .collect();
        tag_compound(
            "block_states",
            &[tag_list("palette", 10, &entries), tag_long_array("data", data)],
        )
    }

    fn str_tag(n: &str, s: &str) -> Vec<u8> {
        let mut out = vec![8u8];
        out.extend(name(n));
        out.extend(be16(s.len() as u16));
        out.extend(s.bytes());
        out
    }

    fn decode_fields(fields: &[Vec<u8>]) -> Value {
        let mut doc = vec![10u8, 0u8, 0u8];
        for f in fields {
            doc.extend(f);
        }
        doc.push(0);
        reader::read(&doc).unwrap()
    }

    fn bare_compound(fields: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for f in fields {
            out.extend(f);
        }
        out.push(0);
        out
    }

    fn bare_string(s: &str) -> Vec<u8> {
        let mut out = be16(s.len() as u16);
        out.extend(s.bytes());
        out
    }

    #[test]
    fn synthetic_one_bit_palette() {
        let data = vec![0xAAAAAAAAAAAAAAAAu64 as i64; 64];
        let fields = vec![
            tag_byte("Y", -64),
            block_states(&["minecraft:air", "minecraft:stone"], &data),
        ];
        let value = decode_fields(&fields);
        let section = decode_section(&value).unwrap();
        assert_eq!(section.y, -64);
        for (i, s) in section.states.iter().enumerate() {
            assert_eq!(*s, (i % 2) as u32);
        }
        assert_eq!(section.states[0], 0);
        assert_eq!(section.states[1], 1);
        assert_eq!(section.states[4095], 1);
        assert!(section.block_light.iter().all(|&b| b == 0));
        assert!(section.sky_light.iter().all(|&b| b == 0));
        assert!(section.biomes.is_none());
    }

    #[test]
    fn synthetic_two_bit_palette() {
        let even = 0x55555555_00000000i64;
        let odd = 0xFFFFFFFF_AAAAAAAAu64 as i64;
        let data: Vec<i64> = (0..128).map(|k| if k % 2 == 0 { even } else { odd }).collect();
        let palette = [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:dirt",
            "minecraft:grass_block",
        ];
        let fields = vec![tag_byte("Y", 0), block_states(&palette, &data)];
        let value = decode_fields(&fields);
        let section = decode_section(&value).unwrap();
        assert_eq!(section.y, 0);
        for (i, s) in section.states.iter().enumerate() {
            assert_eq!(*s, (i / 16 % 4) as u32);
        }
        assert_eq!(section.states[0], 0);
        assert_eq!(section.states[16], 1);
        assert_eq!(section.states[32], 2);
        assert_eq!(section.states[48], 3);
        assert_eq!(section.states[4095], 3);
    }

    fn light_bytes(mut f: impl FnMut(usize) -> i8) -> [i8; 2048] {
        let mut out = [15i8; 2048];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = f(i);
        }
        out
    }

    #[test]
    fn synthetic_lights() {
        let block = light_bytes(|i| match i {
            0 => 0,
            100 => 7,
            _ => 15,
        });
        let sky = light_bytes(|i| if i == 2047 { 12 } else { 15 });
        let data = vec![0x5555555555555555i64; 64];
        let fields = vec![
            tag_byte("Y", 16),
            block_states(&["minecraft:air", "minecraft:stone"], &data),
            tag_byte_array("BlockLight", &block),
            tag_byte_array("SkyLight", &sky),
        ];
        let value = decode_fields(&fields);
        let section = decode_section(&value).unwrap();
        assert_eq!(section.block_light[0], 0);
        assert_eq!(section.block_light[100], 7);
        assert_eq!(section.block_light[101], 15);
        assert_eq!(section.sky_light[2047], 12);
        assert_eq!(section.sky_light[0], 15);
    }

    #[test]
    fn synthetic_biomes() {
        let data = [0xAAAAAAAAAAAAAAAAu64 as i64; 1];
        let entries: Vec<Vec<u8>> =
            ["minecraft:plains", "minecraft:desert"].iter().map(|s| bare_string(s)).collect();
        let fields = vec![
            tag_byte("Y", 32),
            block_states(&["minecraft:air"], &vec![0i64; 64]),
            tag_compound(
                "biomes",
                &[tag_list("palette", 8, &entries), tag_long_array("data", &data)],
            ),
        ];
        let value = decode_fields(&fields);
        let section = decode_section(&value).unwrap();
        let biomes = section.biomes.as_ref().expect("biomes decoded");
        for (i, b) in biomes.iter().enumerate() {
            assert_eq!(*b, (i % 2) as u16);
        }
        assert_eq!(biomes[0], 0);
        assert_eq!(biomes[1], 1);
        assert_eq!(biomes[63], 1);
    }

    #[test]
    fn real_region_sections() {
        let path = format!("{WORLD_REGION_DIR}r.0.0.mca");
        let region = Region::read_region(&path).unwrap_or_else(|e| {
            panic!("real world region files missing at {WORLD_REGION_DIR}: {e}")
        });
        let raw = region.chunk_nbt(0, 0).unwrap();
        let mut out = Vec::new();
        ZlibDecoder::new(&raw[..]).read_to_end(&mut out).unwrap();
        let value = reader::read(&out).unwrap();
        let root = value.as_compound().unwrap();
        let sections = root
            .iter()
            .find(|(k, _)| k == "sections")
            .and_then(|(_, v)| v.as_list())
            .expect("sections list");
        assert!(!sections.is_empty());
        let mut non_air = 0;
        for (idx, entry) in sections.iter().enumerate() {
            let section = decode_section(entry).unwrap();
            if section.states.iter().any(|&s| s != 0) {
                non_air += 1;
            }
            assert!(section.block_light.iter().all(|&b| b <= 15));
            assert!(section.sky_light.iter().all(|&b| b <= 15));
        }
        assert!(non_air >= 1);
    }


    #[test]
    fn errors_missing_fields() {
        let data = vec![0x5555555555555555i64; 64];
        let value = decode_fields(&[block_states(&["minecraft:air", "minecraft:stone"], &data)]);
        match decode_section(&value) {
            Err(Error::Missing) => {}
            other => panic!("expected Missing, got {other:?}"),
        }
        let value = decode_fields(&[tag_byte("Y", 0)]);
        match decode_section(&value) {
            Err(Error::Missing) => {}
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[test]
    fn errors_invalid_data() {
        let value = decode_fields(&[
            tag_byte("Y", 0),
            tag_compound(
                "block_states",
                &[tag_list("palette", 10, &[]), tag_long_array("data", &[0i64; 64])],
            ),
        ]);
        match decode_section(&value) {
            Err(Error::Invalid) => {}
            other => panic!("expected Invalid, got {other:?}"),
        }
        let value = decode_fields(&[
            tag_byte("Y", 0),
            tag_compound(
                "block_states",
                &[
                    tag_list(
                        "palette",
                        10,
                        &[bare_compound(&[str_tag("Name", "minecraft:air")])],
                    ),
                    tag_long_array("data", &vec![0x5555555555555555i64; 128]),
                ],
            ),
        ]);
        match decode_section(&value) {
            Err(Error::Invalid) => {}
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
