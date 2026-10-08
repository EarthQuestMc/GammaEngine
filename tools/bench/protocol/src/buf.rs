//! Big-endian primitives and Minecraft strings, read from a slice and written to a `Vec<u8>`.

use crate::varint;
use crate::{Error, Result};

/// Longest string `PacketBuffer.writeStringToBuffer` accepts, in UTF-8 bytes.
pub const MAX_STRING_BYTES: usize = 32767;

/// A cursor over a packet body.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Fails with [`Error::TrailingBytes`] unless every byte has been read.
    pub fn finish(&self) -> Result<()> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(Error::TrailingBytes(n)),
        }
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(Error::UnexpectedEof);
        }
        let bytes = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(bytes)
    }

    /// Everything not read yet.
    pub fn read_rest(&mut self) -> &'a [u8] {
        let rest = &self.buf[self.pos..];
        self.pos = self.buf.len();
        rest
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.read_bytes(n).map(|_| ())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.read_bytes(N)?);
        Ok(out)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// Any non-zero byte is true, as in `readBoolean` and `readUnsignedByte() != 0`.
    pub fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array()?))
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.array()?))
    }

    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.array()?))
    }

    pub fn read_varint(&mut self) -> Result<i32> {
        self.read_varint_max(varint::MAX_VARINT_LEN)
    }

    /// A VarInt limited to `max_len` bytes (Forge's `ByteBufUtils.readVarInt(buf, maxSize)`).
    pub fn read_varint_max(&mut self, max_len: usize) -> Result<i32> {
        match varint::decode_varint_max(&self.buf[self.pos..], max_len)? {
            Some((value, len)) => {
                self.pos += len;
                Ok(value)
            }
            None => Err(Error::UnexpectedEof),
        }
    }

    pub fn read_varlong(&mut self) -> Result<i64> {
        match varint::decode_varlong(&self.buf[self.pos..])? {
            Some((value, len)) => {
                self.pos += len;
                Ok(value)
            }
            None => Err(Error::UnexpectedEof),
        }
    }

    /// A Minecraft string: VarInt byte length, then UTF-8.
    ///
    /// Applies the two checks of `PacketBuffer.readStringFromBuffer(max)`: at most `4 * max`
    /// bytes, then at most `max` UTF-16 units once decoded.
    pub fn read_string(&mut self, max_chars: usize) -> Result<String> {
        let len = self.read_varint()?;
        if len < 0 {
            return Err(Error::NegativeLength(i64::from(len)));
        }
        let len = len as usize;
        if len > max_chars.saturating_mul(4) {
            return Err(Error::StringTooLong {
                max: max_chars * 4,
                len,
            });
        }
        let text = std::str::from_utf8(self.read_bytes(len)?).map_err(|_| Error::InvalidUtf8)?;
        let units = text.encode_utf16().count();
        if units > max_chars {
            return Err(Error::StringTooLong {
                max: max_chars,
                len: units,
            });
        }
        Ok(text.to_owned())
    }
}

/// Builds a packet body.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Writer {
            buf: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn write_u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn write_i8(&mut self, value: i8) {
        self.buf.push(value as u8);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.buf.push(u8::from(value));
    }

    pub fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_i16(&mut self, value: i16) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_i32(&mut self, value: i32) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_i64(&mut self, value: i64) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_f32(&mut self, value: f32) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_f64(&mut self, value: f64) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_varint(&mut self, value: i32) {
        varint::write_varint(&mut self.buf, value);
    }

    pub fn write_varlong(&mut self, value: i64) {
        varint::write_varlong(&mut self.buf, value);
    }

    /// A Minecraft string. Fails above 32767 UTF-8 bytes, like `writeStringToBuffer`.
    pub fn write_string(&mut self, text: &str) -> Result<()> {
        if text.len() > MAX_STRING_BYTES {
            return Err(Error::StringTooLong {
                max: MAX_STRING_BYTES,
                len: text.len(),
            });
        }
        self.write_varint(text.len() as i32);
        self.write_bytes(text.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_big_endian() {
        let mut w = Writer::new();
        w.write_u8(0xab);
        w.write_bool(true);
        w.write_i16(-2);
        w.write_u16(25565);
        w.write_i32(0x0102_0304);
        w.write_i64(-1);
        w.write_f32(1.5);
        w.write_f64(-0.25);
        let bytes = w.into_inner();
        assert_eq!(
            &bytes[..10],
            &[0xab, 0x01, 0xff, 0xfe, 0x63, 0xdd, 0x01, 0x02, 0x03, 0x04]
        );

        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_u8(), Ok(0xab));
        assert_eq!(r.read_bool(), Ok(true));
        assert_eq!(r.read_i16(), Ok(-2));
        assert_eq!(r.read_u16(), Ok(25565));
        assert_eq!(r.read_i32(), Ok(0x0102_0304));
        assert_eq!(r.read_i64(), Ok(-1));
        assert_eq!(r.read_f32(), Ok(1.5));
        assert_eq!(r.read_f64(), Ok(-0.25));
        assert!(r.finish().is_ok());
        assert_eq!(r.read_u8(), Err(Error::UnexpectedEof));
    }

    #[test]
    fn strings_round_trip() {
        for text in ["", "bot1", "Été à Saint-Malo", "\u{1F600} smile", "a\0b"] {
            let mut w = Writer::new();
            w.write_string(text).unwrap();
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(r.read_string(32767).as_deref(), Ok(text));
            assert!(r.finish().is_ok());
        }
    }

    #[test]
    fn string_length_prefix_counts_bytes() {
        let mut w = Writer::new();
        w.write_string("é").unwrap();
        assert_eq!(w.as_slice(), &[0x02, 0xc3, 0xa9]);
    }

    #[test]
    fn string_limits_follow_the_server() {
        let mut w = Writer::new();
        w.write_string("abcdefghijklmnopq").unwrap();
        let bytes = w.into_inner();
        // 17 characters against the 16 of a login name.
        assert_eq!(
            Reader::new(&bytes).read_string(16),
            Err(Error::StringTooLong { max: 16, len: 17 })
        );
        // The byte check comes first: 17 bytes against 4 * 4.
        assert_eq!(
            Reader::new(&bytes).read_string(4),
            Err(Error::StringTooLong { max: 16, len: 17 })
        );
        // A surrogate pair counts as two UTF-16 units, as in Java.
        let mut w = Writer::new();
        w.write_string("\u{1F600}").unwrap();
        assert_eq!(
            Reader::new(w.as_slice()).read_string(1),
            Err(Error::StringTooLong { max: 1, len: 2 })
        );
    }

    #[test]
    fn string_errors() {
        assert_eq!(
            Reader::new(&[0x03, b'a']).read_string(10),
            Err(Error::UnexpectedEof)
        );
        assert_eq!(
            Reader::new(&[0x02, 0xc3, 0x28]).read_string(10),
            Err(Error::InvalidUtf8)
        );
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]).read_string(10),
            Err(Error::NegativeLength(-1))
        );
        let long = "x".repeat(MAX_STRING_BYTES + 1);
        assert!(Writer::new().write_string(&long).is_err());
    }

    #[test]
    fn trailing_bytes_are_reported() {
        let mut r = Reader::new(&[1, 2, 3]);
        r.read_u8().unwrap();
        assert_eq!(r.finish(), Err(Error::TrailingBytes(2)));
        assert_eq!(r.read_rest(), &[2, 3]);
        assert!(r.finish().is_ok());
    }
}
