use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::DeflateDecoder;

const EOCD_SIG: u32 = 0x06054b50;
const CD_SIG: u32 = 0x02014b50;
const LOCAL_HEADER: usize = 30;
const EOCD_SIZE: usize = 22;
const CD_BASE: usize = 46;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

fn u16le(d: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([d[p], d[p + 1]])
}

fn u32le(d: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    NotAZip,
    InvalidName,
    EntryNotFound,
    UnsupportedMethod(u16),
    Decompression,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::NotAZip => write!(f, "not a zip file"),
            Error::InvalidName => write!(f, "invalid entry name"),
            Error::EntryNotFound => write!(f, "entry not found"),
            Error::UnsupportedMethod(m) => write!(f, "unsupported compression method {m}"),
            Error::Decompression => write!(f, "decompression failed"),
        }
    }
}

impl std::error::Error for Error {}

struct Entry {
    method: u16,
    data: usize,
    size: usize,
}

pub struct ZipFile {
    data: Vec<u8>,
    entries: HashMap<Vec<u8>, Entry>,
}

impl ZipFile {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<ZipFile, Error> {
        let data = fs::read(path.as_ref()).map_err(Error::Io)?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<ZipFile, Error> {
        let eocd = Self::find_eocd(&data).ok_or(Error::NotAZip)?;
        let count = u16le(&data, eocd + 10) as usize;
        let mut cd = u32le(&data, eocd + 16) as usize;
        if cd + EOCD_SIZE > data.len() {
            return Err(Error::NotAZip);
        }
        let mut entries: HashMap<Vec<u8>, Entry> = HashMap::new();
        for _ in 0..count {
            if cd + CD_BASE > data.len() || u32le(&data, cd) != CD_SIG {
                return Err(Error::NotAZip);
            }
            let method = u16le(&data, cd + 10);
            let comp_size = u32le(&data, cd + 20) as usize;
            let name_len = u16le(&data, cd + 28) as usize;
            let extra_len = u16le(&data, cd + 30) as usize;
            let comment_len = u16le(&data, cd + 32) as usize;
            let local_off = u32le(&data, cd + 42) as usize;
            let name_end = cd + CD_BASE + name_len;
            if name_end > data.len() {
                return Err(Error::NotAZip);
            }
            let name = &data[cd + CD_BASE..name_end];
            let data_start = Self::local_header_data_start(local_off, &data).ok_or(Error::NotAZip)?;
            if data_start + comp_size > data.len() {
                return Err(Error::NotAZip);
            }
            entries.insert(name.to_vec(), Entry {
                method,
                data: data_start,
                size: comp_size,
            });
            cd = cd + CD_BASE + name_len + extra_len + comment_len;
        }
        if entries.is_empty() {
            return Err(Error::NotAZip);
        }
        Ok(ZipFile { data, entries })
    }

    fn find_eocd(data: &[u8]) -> Option<usize> {
        if data.len() < EOCD_SIZE {
            return None;
        }
        let last = data.len() - EOCD_SIZE;
        let first = last.saturating_sub(65535);
        let mut found = None;
        for p in first..=last {
            if u32le(data, p) == EOCD_SIG {
                found = Some(p);
            }
        }
        found
    }

    fn local_header_data_start(local_off: usize, data: &[u8]) -> Option<usize> {
        let p = local_off.checked_add(LOCAL_HEADER)?;
        if p > data.len() {
            return None;
        }
        let name_len = u16le(data, local_off + 26) as usize;
        let extra_len = u16le(data, local_off + 28) as usize;
        Some(p + name_len + extra_len)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name.as_bytes())
    }

    pub fn entry_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .entries
            .keys()
            .filter_map(|n| std::str::from_utf8(n).ok())
            .collect();
        names.sort_unstable();
        names
    }

    pub fn read_entry(&self, name: &str) -> Result<Vec<u8>, Error> {
        let entry = self.entries.get(name.as_bytes()).ok_or(Error::EntryNotFound)?;
        let raw = &self.data[entry.data..entry.data + entry.size];
        match entry.method {
            METHOD_STORED => Ok(raw.to_vec()),
            METHOD_DEFLATE => {
                let mut decoder = DeflateDecoder::new(raw);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|_| Error::Decompression)?;
                Ok(out)
            }
            method => Err(Error::UnsupportedMethod(method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const LOCAL_SIG: u32 = 0x04034b50;

    fn local_header(name: &str, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; LOCAL_HEADER];
        h[0..4].copy_from_slice(&LOCAL_SIG.to_le_bytes());
        h[18..22].copy_from_slice(&(data.len() as u32).to_le_bytes());
        h[22..26].copy_from_slice(&(data.len() as u32).to_le_bytes());
        h[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
        h.extend_from_slice(name.as_bytes());
        h
    }

    fn central_entry(
        name: &str,
        method: u16,
        comp: &[u8],
        uncompressed: usize,
        local_off: usize,
    ) -> Vec<u8> {
        let mut e = vec![0u8; CD_BASE];
        e[0..4].copy_from_slice(&CD_SIG.to_le_bytes());
        e[10..12].copy_from_slice(&method.to_le_bytes());
        e[20..24].copy_from_slice(&(comp.len() as u32).to_le_bytes());
        e[24..28].copy_from_slice(&(uncompressed as u32).to_le_bytes());
        e[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        e[42..46].copy_from_slice(&(local_off as u32).to_le_bytes());
        e.extend_from_slice(name.as_bytes());
        e
    }

    fn eocd(count: u16, cd_size: u32, cd_off: u32) -> Vec<u8> {
        let mut e = vec![0u8; EOCD_SIZE];
        e[0..4].copy_from_slice(&EOCD_SIG.to_le_bytes());
        e[10..12].copy_from_slice(&count.to_le_bytes());
        e[12..16].copy_from_slice(&cd_size.to_le_bytes());
        e[16..20].copy_from_slice(&cd_off.to_le_bytes());
        e
    }

    fn deflate(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::DeflateEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        );
        encoder.write_all(data).expect("write deflate");
        encoder.finish().expect("finish deflate")
    }

    #[test]
    fn reads_stored_and_deflated_entries() {
        let stored = b"hello stored";
        let payload = b"hello deflate, hello deflate".repeat(4);
        let deflated = deflate(&payload);
        let local_a = local_header("a.txt", stored);
        let local_b = local_header("b.txt", &deflated);
        let mut cd = Vec::new();
        cd.extend_from_slice(&central_entry("a.txt", METHOD_STORED, stored, stored.len(), 0));
        cd.extend_from_slice(&central_entry(
            "b.txt",
            METHOD_DEFLATE,
            &deflated,
            payload.len(),
            local_a.len() + stored.len(),
        ));
        let mut zip = Vec::new();
        zip.extend_from_slice(&local_a);
        zip.extend_from_slice(stored);
        zip.extend_from_slice(&local_b);
        zip.extend_from_slice(&deflated);
        let cd_off = zip.len() as u32;
        zip.extend_from_slice(&cd);
        zip.extend_from_slice(&eocd(2, cd.len() as u32, cd_off));

        let zip = ZipFile::from_bytes(zip).expect("parse zip");
        assert!(zip.contains("a.txt"));
        assert!(zip.contains("b.txt"));
        assert!(!zip.contains("c.txt"));
        assert_eq!(zip.read_entry("a.txt").expect("read a"), stored.to_vec());
        assert_eq!(zip.read_entry("b.txt").expect("read b"), payload.to_vec());
        assert!(matches!(zip.read_entry("c.txt"), Err(Error::EntryNotFound)));
        assert_eq!(zip.entry_names(), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn rejects_non_zip() {
        let data = b"definitely not a zip file, just some text";
        assert!(matches!(ZipFile::from_bytes(data.to_vec()), Err(Error::NotAZip)));
    }

    #[test]
    fn reads_blockstate_from_real_jar() {
        const JAR: &str =
            "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";
        let zip = ZipFile::from_path(JAR).expect("open jar");
        let data = zip
            .read_entry("assets/minecraft/blockstates/stone.json")
            .expect("read stone.json");
        assert!(data.first() == Some(&b'{'));
    }
}
