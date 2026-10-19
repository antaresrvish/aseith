use std::fmt;
use std::fs;
use std::path::Path;

const HEADER: usize = 8192;
const SECTOR: usize = 4096;

#[derive(Debug)]
pub enum Error {
    #[allow(dead_code)]
    Io(std::io::Error),
    InvalidHeader,
    EmptySlot,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::InvalidHeader => write!(f, "invalid region header"),
            Error::EmptySlot => write!(f, "no chunk in slot"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Region {
    data: Vec<u8>,
}

impl Region {
    #[allow(dead_code)]
    pub fn read_region<P: AsRef<Path>>(path: P) -> Result<Region, Error> {
        let data = fs::read(path.as_ref()).map_err(Error::Io)?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Region, Error> {
        if data.len() < HEADER {
            return Err(Error::InvalidHeader);
        }
        Ok(Region { data })
    }

    pub fn slot(&self, cx: u8, cz: u8) -> Option<(usize, usize)> {
        if cx > 31 || cz > 31 {
            return None;
        }
        let entry = self.offset_entry((cz as usize) * 32 + cx as usize);
        if entry == 0 {
            return None;
        }
        let sectors = ((entry >> 24) & 0xFF) as usize;
        let units = ((entry >> 16) & 0xFF) as usize | ((((entry >> 8) & 0xFF) as usize) << 8);
        if sectors == 0 {
            return None;
        }
        let offset = HEADER + units * SECTOR;
        let size = sectors * SECTOR;
        if offset + size > self.data.len() {
            return None;
        }
        Some((offset, size))
    }

    pub fn timestamp(&self, cx: u8, cz: u8) -> Option<i32> {
        if cx > 31 || cz > 31 {
            return None;
        }
        if self.slot(cx, cz).is_none() {
            return None;
        }
        let index = (cz as usize) * 32 + cx as usize;
        let p = HEADER / 2 + index * 4;
        Some(
            u32::from_le_bytes([
                self.data[p],
                self.data[p + 1],
                self.data[p + 2],
                self.data[p + 3],
            ]) as i32,
        )
    }

    pub fn chunk_nbt(&self, cx: u8, cz: u8) -> Result<Vec<u8>, Error> {
        let (offset, size) = self.slot(cx, cz).ok_or(Error::EmptySlot)?;
        let block = &self.data[offset..offset + size];
        if block[4] == 0 {
            return Err(Error::InvalidHeader);
        }
        Ok(block[5..].to_vec())
    }

    fn offset_entry(&self, index: usize) -> u32 {
        let p = index * 4;
        u32::from_le_bytes([
            self.data[p],
            self.data[p + 1],
            self.data[p + 2],
            self.data[p + 3],
        ])
    }
}

pub fn parse_region_name(name: &str) -> Option<(i32, i32)> {
    let name = name.rsplit('/').next().unwrap_or(name);
    let name = name.strip_suffix(".mca")?;
    let name = name.strip_prefix("r.")?;
    let dot = name.rfind('.')?;
    let rx: i32 = name[..dot].parse().ok()?;
    let rz: i32 = name[dot + 1..].parse().ok()?;
    Some((rx, rz))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    use std::io::Write;

    const WORLD_REGION_DIR: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/instances/aseith/minecraft/saves/anvil/region/";

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        );
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn build_region(chunks: &[(u8, u8, &[u8], i32)]) -> Vec<u8> {
        let mut offsets = [0u32; 1024];
        let mut stamps = [0u32; 1024];
        let mut body = Vec::new();
        let mut unit = 0usize;
        for (cx, cz, payload, ts) in chunks {
            let zlib = zlib(payload);
            let sectors = ((zlib.len() + 4 + SECTOR - 1) / SECTOR).max(1);
            let mut block = vec![0u8; sectors * SECTOR];
            block[2] = ((zlib.len() + 1) >> 8) as u8;
            block[3] = (zlib.len() + 1) as u8;
            block[4] = sectors as u8;
            block[5..5 + zlib.len()].copy_from_slice(&zlib);
            let index = *cz as usize * 32 + *cx as usize;
            offsets[index] = ((sectors as u32) << 24) | (((unit as u32) & 0x0FFF) << 8);
            stamps[index] = *ts as u32;
            body.extend_from_slice(&block);
            unit += sectors;
        }
        let mut data = vec![0u8; HEADER];
        for i in 0..1024 {
            data[i * 4..i * 4 + 4].copy_from_slice(&offsets[i].to_le_bytes());
            data[HEADER / 2 + i * 4..HEADER / 2 + i * 4 + 4]
                .copy_from_slice(&stamps[i].to_le_bytes());
        }
        data.extend_from_slice(&body);
        data
    }

    fn nbt_bytes() -> Vec<u8> {
        let mut v = vec![10u8, 0u8, 0u8];
        v.push(3);
        v.extend_from_slice(&4u16.to_be_bytes());
        v.extend_from_slice(b"xPos");
        v.extend_from_slice(&7i32.to_be_bytes());
        v.push(3);
        v.extend_from_slice(&4u16.to_be_bytes());
        v.extend_from_slice(b"zPos");
        v.extend_from_slice(&9i32.to_be_bytes());
        v.push(0);
        v
    }

    #[test]
    fn synthetic_slot_and_nbt() {
        let payload = nbt_bytes();
        let data = build_region(&[(3, 5, &payload, 1234)]);
        let region = Region::from_bytes(data).unwrap();
        let (offset, size) = region.slot(3, 5).unwrap();
        assert_eq!(offset, HEADER);
        assert_eq!(size, SECTOR);
        assert_eq!(region.timestamp(3, 5), Some(1234));
        let raw = region.chunk_nbt(3, 5).unwrap();
        let expected = zlib(&payload);
        assert_eq!(&raw[..expected.len()], expected.as_slice());
        assert!(region.slot(0, 0).is_none());
        assert!(region.slot(31, 31).is_none());
        assert_eq!(region.timestamp(0, 0), None);
        match region.chunk_nbt(0, 0) {
            Err(Error::EmptySlot) => {}
            other => panic!("expected EmptySlot, got {other:?}"),
        }
    }

    #[test]
    fn multi_sector_block() {
        let mut state = 0x9E3779B97F4A7C15u64;
        let payload: Vec<u8> = (0..100_000)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 40) as u8
            })
            .collect();
        let data = build_region(&[(7, 2, &payload, 99)]);
        let region = Region::from_bytes(data).unwrap();
        let (offset, size) = region.slot(7, 2).unwrap();
        assert!(size > SECTOR);
        assert_eq!(offset + size, region.data.len());
        let raw = region.chunk_nbt(7, 2).unwrap();
        let expected = zlib(&payload);
        assert_eq!(&raw[..expected.len()], expected.as_slice());
    }

    #[test]
    fn real_region_chunks() {
        let path = format!("{WORLD_REGION_DIR}r.0.0.mca");
        let region = Region::read_region(&path).unwrap_or_else(|e| {
            panic!("real world region files missing at {WORLD_REGION_DIR}: {e}")
        });
        let mut non_empty = 0;
        for cz in 0..32 {
            for cx in 0..32 {
                if region.slot(cx, cz).is_some() {
                    non_empty += 1;
                }
            }
        }
        assert!(non_empty >= 1);
        let (cx, cz) = (0u8, 0u8);
        let raw = region.chunk_nbt(cx, cz).unwrap();
        assert_eq!(&raw[0..2], &[0x78, 0x9c]);
        let mut out = Vec::new();
        ZlibDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .unwrap();
        let value = crate::nbt::reader::read(&out).unwrap();
        let root = value.as_compound().unwrap();
        assert!(root.iter().any(|(k, _)| k.contains("xPos")));
        assert!(root.iter().any(|(k, _)| k.contains("zPos")));
    }

    #[test]
    fn parse_region_names() {
        assert_eq!(parse_region_name("r.0.0.mca"), Some((0, 0)));
        assert_eq!(parse_region_name("r.-1.0.mca"), Some((-1, 0)));
        assert_eq!(parse_region_name("r.0.-1.mca"), Some((0, -1)));
        assert_eq!(parse_region_name("region/r.0.0.mca"), Some((0, 0)));
        assert_eq!(parse_region_name("region/foo.mca"), None);
        assert_eq!(parse_region_name("foo.mca"), None);
        assert_eq!(parse_region_name("r.0.mca"), None);
    }
}
