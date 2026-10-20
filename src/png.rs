use std::io::Read;
use flate2::read::ZlibDecoder;

#[derive(Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug)]
pub enum Error {
    NotAPng,
    BadIhdr,
    BitDepth(u8),
    ColorType(u8),
    Interlaced,
    Truncated,
    BadFilter(u8),
    Inflate(std::io::Error),
}

pub fn decode(data: &[u8]) -> Result<Image, Error> {
    let sig = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < 8 || &data[..8] != &sig[..] {
        return Err(Error::NotAPng);
    }
    let mut pos = 8;
    let mut size: Option<(u32, u32)> = None;
    let mut idat: Vec<u8> = Vec::new();
    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes([
            data[pos],
            data[pos + 1],
            data[pos + 2],
            data[pos + 3],
        ]) as usize;
        let name = &data[pos + 4..pos + 8];
        if pos + 8 + len + 4 > data.len() {
            return Err(Error::Truncated);
        }
        let body = &data[pos + 8..pos + 8 + len];
        if name == b"IHDR" {
            size = Some(read_ihdr(body)?);
        } else if name == b"IDAT" {
            idat.extend_from_slice(body);
        } else if name == b"IEND" {
            break;
        }
        pos += 12 + len;
    }
    let (w, h) = size.ok_or(Error::Truncated)?;
    inflate(w, h, &idat)
}

fn read_ihdr(body: &[u8]) -> Result<(u32, u32), Error> {
    if body.len() < 13 {
        return Err(Error::BadIhdr);
    }
    let w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    let h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
    if body[8] != 8 {
        return Err(Error::BitDepth(body[8]));
    }
    if body[9] != 6 {
        return Err(Error::ColorType(body[9]));
    }
    if body[12] != 0 {
        return Err(Error::Interlaced);
    }
    Ok((w, h))
}

fn inflate(w: u32, h: u32, idat: &[u8]) -> Result<Image, Error> {
    let stride = (w * 4) as usize;
    let expected = (stride + 1) * h as usize;
    let mut decoder = ZlibDecoder::new(idat);
    let mut raw = Vec::with_capacity(expected);
    decoder.read_to_end(&mut raw).map_err(Error::Inflate)?;
    if raw.len() < expected {
        return Err(Error::Truncated);
    }
    let mut pixels = vec![0u8; stride * h as usize];
    let mut prev: Vec<u8> = vec![0; stride];
    let mut pos = 0;
    for y in 0..h as usize {
        let f = raw[pos];
        pos += 1;
        let line = &mut pixels[y * stride..(y + 1) * stride];
        defilter_line(f, &raw[pos..pos + stride], line, &prev, 4)?;
        pos += stride;
        prev.copy_from_slice(line);
    }
    Ok(Image { width: w, height: h, pixels })
}

fn defilter_line(
    f: u8,
    src: &[u8],
    line: &mut [u8],
    prev: &[u8],
    bpp: usize,
) -> Result<(), Error> {
    if src.len() != line.len() || prev.len() != line.len() {
        return Err(Error::Truncated);
    }
    match f {
        0 => line.copy_from_slice(src),
        1 => {
            for i in 0..line.len() {
                let a = if i >= bpp { line[i - bpp] } else { 0 };
                line[i] = src[i].wrapping_add(a);
            }
        }
        2 => {
            for i in 0..line.len() {
                line[i] = src[i].wrapping_add(prev[i]);
            }
        }
        3 => {
            for i in 0..line.len() {
                let a = if i >= bpp { line[i - bpp] } else { 0 };
                line[i] = src[i].wrapping_add(((a as u32 + prev[i] as u32) / 2) as u8);
            }
        }
        4 => {
            for i in 0..line.len() {
                let a = if i >= bpp { line[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                let p = (a as u32) + (b as u32) + (c as u32);
                let pa = p.abs_diff(a as u32);
                let pb = p.abs_diff(b as u32);
                let pc = p.abs_diff(c as u32);
                let pr = if pa <= pb && pa <= pc {
                    a as u32
                } else if pb <= pc {
                    b as u32
                } else {
                    c as u32
                };
                line[i] = src[i].wrapping_add(pr as u8);
            }
        }
        _ => return Err(Error::BadFilter(f)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use crate::zipfile::ZipFile;

    const JAR: &str = "/Users/antaresrvish/Library/Application Support/PrismLauncher/libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar";

    #[test]
    fn stone_decodes() {
        let zf = ZipFile::from_path(JAR).expect("open jar");
        let data = zf
            .read_entry("assets/minecraft/textures/block/stone.png")
            .expect("read stone.png");
        let img = decode(&data).expect("decode stone.png");
        assert_eq!(img.width, 16);
        assert_eq!(img.height, 16);
        assert_eq!(img.pixels.len(), 16 * 16 * 4);
        assert_eq!(&img.pixels[..4], &[143, 143, 143, 255]);
        assert_eq!(&img.pixels[15 * 4..16 * 4], &[127, 127, 127, 255]);
    }

    #[test]
    fn garbage_rejected() {
        assert!(matches!(decode(b"not a png at all"), Err(Error::NotAPng)));
        assert!(matches!(decode(&[]), Err(Error::NotAPng)));
    }

    #[test]
    fn truncated_rejected() {
        let zf = ZipFile::from_path(JAR).expect("open jar");
        let data = zf
            .read_entry("assets/minecraft/textures/block/stone.png")
            .expect("read stone.png");
        let err = decode(&data[..data.len() - 30]).expect_err("truncated");
        assert!(matches!(err, Error::Truncated | Error::Inflate(_)));
    }

    fn make_png(w: u32, h: u32, depth: u8, color: u8, interlace: u8, raw: &[u8]) -> Vec<u8> {
        let mut ihdr: Vec<u8> = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.push(depth);
        ihdr.push(color);
        ihdr.push(0);
        ihdr.push(0);
        ihdr.push(interlace);
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(raw).unwrap();
        let idat = enc.finish().unwrap();
        let mut out: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        for (name, body) in [(b"IHDR", &ihdr[..]), (b"IDAT", &idat[..])] {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(name);
            out.extend_from_slice(body);
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&0u32.to_be_bytes());
        out
    }

    #[test]
    fn ihdr_variants_rejected() {
        let raw = [0u8, 1, 2, 3, 4, 0, 5, 6, 7, 8, 0, 9, 10, 11, 12];
        let bad = make_png(1, 1, 5, 6, 0, &raw);
        assert!(matches!(decode(&bad), Err(Error::BitDepth(5))));
        let bad = make_png(1, 1, 8, 2, 0, &raw);
        assert!(matches!(decode(&bad), Err(Error::ColorType(2))));
        let bad = make_png(1, 1, 8, 6, 1, &raw);
        assert!(matches!(decode(&bad), Err(Error::Interlaced)));
    }

    #[test]
    fn filters_roundtrip() {
        let w = 2u32;
        let h = 5u32;
        let stride = (w * 4) as usize;
        let cur: Vec<u8> = (0..w * h * 4).map(|i| (i * 37 + 11) as u8).collect();
        let mut prev: Vec<u8> = vec![0; stride];
        let mut raw: Vec<u8> = Vec::new();
        for y in 0..h as usize {
            let f = y as u8;
            let line = &cur[y * stride..(y + 1) * stride];
            raw.push(f);
            for i in 0..stride {
                let a = if i >= 4 { line[i - 4] } else { 0 };
                let b = prev[i];
                let c = if i >= 4 { prev[i - 4] } else { 0 };
                let src = match f {
                    0 => line[i],
                    1 => line[i].wrapping_sub(a),
                    2 => line[i].wrapping_sub(b),
                    3 => line[i].wrapping_sub(((a as u32 + b as u32) / 2) as u8),
                    _ => {
                        let p = a as u32 + b as u32 + c as u32;
                        let pa = p.abs_diff(a as u32);
                        let pb = p.abs_diff(b as u32);
                        let pc = p.abs_diff(c as u32);
                        let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                        line[i].wrapping_sub(pr)
                    }
                };
                raw.push(src);
            }
            prev.copy_from_slice(line);
        }
        let data = make_png(w, h, 8, 6, 0, &raw);
        let img = decode(&data).expect("decode synthetic");
        assert_eq!(img.pixels, cur);
    }
}
