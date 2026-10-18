use std::io::Read;

use flate2::read::GzDecoder;

use crate::nbt::error::Error;
use crate::nbt::value::Value;

const MAX_DEPTH: usize = 128;

pub fn read(data: &[u8]) -> Result<Value, Error> {
    let mut reader = Reader { data, pos: 0 };
    let id = reader.byte()?;
    reader.string()?;
    reader.tag(id, 0)
}

pub fn read_gzip(data: &[u8]) -> Result<Value, Error> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).map_err(|_| Error::InvalidGzip)?;
    read(&decompressed)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = self.data.get(self.pos).ok_or(Error::Truncated)?;
        self.pos += 1;
        Ok(*b)
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], Error> {
        if self.pos + len > self.data.len() {
            return Err(Error::Truncated);
        }
        let chunk = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(chunk)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let b = self.bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let b = self.bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let b = self.bytes(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i8(&mut self) -> Result<i8, Error> {
        Ok(self.byte()? as i8)
    }

    fn i16(&mut self) -> Result<i16, Error> {
        Ok(self.u16()? as i16)
    }

    fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }

    fn i64(&mut self) -> Result<i64, Error> {
        Ok(self.u64()? as i64)
    }

    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn f64(&mut self) -> Result<f64, Error> {
        Ok(f64::from_bits(self.u64()?))
    }

    fn string(&mut self) -> Result<String, Error> {
        let len = self.u16()? as usize;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    fn array_count(&mut self, element_size: usize) -> Result<usize, Error> {
        let count = self.i32()?;
        if count < 0 {
            return Err(Error::InvalidCount);
        }
        let count = count as usize;
        if count as i64 * element_size as i64 > (self.data.len() - self.pos) as i64 {
            return Err(Error::Truncated);
        }
        Ok(count)
    }

    fn list_count(&mut self) -> Result<usize, Error> {
        let count = self.i32()?;
        if count < 0 {
            return Err(Error::InvalidCount);
        }
        let count = count as usize;
        if count > self.data.len() - self.pos {
            return Err(Error::Truncated);
        }
        Ok(count)
    }

    fn tag(&mut self, id: u8, depth: usize) -> Result<Value, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TooDeep);
        }
        match id {
            0 => Ok(Value::End),
            1 => Ok(Value::Byte(self.i8()?)),
            2 => Ok(Value::Short(self.i16()?)),
            3 => Ok(Value::Int(self.i32()?)),
            4 => Ok(Value::Long(self.i64()?)),
            5 => Ok(Value::Float(self.f32()?)),
            6 => Ok(Value::Double(self.f64()?)),
            7 => {
                let count = self.array_count(1)?;
                let bytes = self.bytes(count)?;
                let items = bytes.iter().map(|b| *b as i8).collect();
                Ok(Value::ByteArray(items))
            }
            8 => Ok(Value::String(self.string()?)),
            9 => {
                let element = self.byte()?;
                let count = self.list_count()?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.tag(element, depth + 1)?);
                }
                Ok(Value::List { element, items })
            }
            10 => {
                let mut fields = Vec::new();
                loop {
                    let id = self.byte()?;
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    fields.push((name, self.tag(id, depth + 1)?));
                }
                Ok(Value::Compound(fields))
            }
            11 => {
                let count = self.array_count(4)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.i32()?);
                }
                Ok(Value::IntArray(items))
            }
            12 => {
                let count = self.array_count(8)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.i64()?);
                }
                Ok(Value::LongArray(items))
            }
            id => Err(Error::UnknownType(id)),
        }
    }
}

#[cfg(test)]
mod tests;
