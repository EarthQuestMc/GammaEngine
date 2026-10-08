//! VarInt and VarLong: 7 bits per byte, least significant group first, high bit set on every
//! byte but the last. Negative values are encoded as their two's complement, so they always take
//! the maximum length.

use crate::{Error, Result};

pub const MAX_VARINT_LEN: usize = 5;
pub const MAX_VARLONG_LEN: usize = 10;

/// Number of bytes [`write_varint`] produces for `value`.
pub fn varint_len(value: i32) -> usize {
    match value as u32 {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x1f_ffff => 3,
        0x20_0000..=0x0fff_ffff => 4,
        _ => 5,
    }
}

/// Number of bytes [`write_varlong`] produces for `value`.
pub fn varlong_len(value: i64) -> usize {
    let bits = 64 - (value as u64).leading_zeros() as usize;
    (bits.max(1) + 6) / 7
}

pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn write_varlong(out: &mut Vec<u8>, value: i64) {
    let mut v = value as u64;
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Decodes a VarInt at the start of `buf`.
///
/// Returns the value and the number of bytes it used, `Ok(None)` when `buf` ends inside the
/// value, and an error when the value runs past five bytes.
pub fn decode_varint(buf: &[u8]) -> Result<Option<(i32, usize)>> {
    decode_varint_max(buf, MAX_VARINT_LEN)
}

/// Like [`decode_varint`] with a tighter byte budget, as Forge's `ByteBufUtils.readVarInt` does.
///
/// Bits beyond 32 are dropped, as Java's `int` shift drops them.
pub fn decode_varint_max(buf: &[u8], max_len: usize) -> Result<Option<(i32, usize)>> {
    let max_len = max_len.min(MAX_VARINT_LEN);
    let mut value: u32 = 0;
    for i in 0..max_len {
        let Some(&byte) = buf.get(i) else {
            return Ok(None);
        };
        value |= u32::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(Some((value as i32, i + 1)));
        }
    }
    Err(Error::VarIntTooLong)
}

/// Decodes a VarLong at the start of `buf`, with the same conventions as [`decode_varint`].
pub fn decode_varlong(buf: &[u8]) -> Result<Option<(i64, usize)>> {
    let mut value: u64 = 0;
    for i in 0..MAX_VARLONG_LEN {
        let Some(&byte) = buf.get(i) else {
            return Ok(None);
        };
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(Some((value as i64, i + 1)));
        }
    }
    Err(Error::VarIntTooLong)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(value: i32) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, value);
        out
    }

    fn encode_long(value: i64) -> Vec<u8> {
        let mut out = Vec::new();
        write_varlong(&mut out, value);
        out
    }

    #[test]
    fn varint_known_encodings() {
        // Reference values from the protocol documentation.
        let cases: &[(i32, &[u8])] = &[
            (0, &[0x00]),
            (1, &[0x01]),
            (2, &[0x02]),
            (127, &[0x7f]),
            (128, &[0x80, 0x01]),
            (255, &[0xff, 0x01]),
            (25565, &[0xdd, 0xc7, 0x01]),
            (2_097_151, &[0xff, 0xff, 0x7f]),
            (2_147_483_647, &[0xff, 0xff, 0xff, 0xff, 0x07]),
            (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
            (-2_147_483_648, &[0x80, 0x80, 0x80, 0x80, 0x08]),
        ];
        for &(value, bytes) in cases {
            assert_eq!(encode(value), bytes, "encoding {value}");
            assert_eq!(varint_len(value), bytes.len(), "length of {value}");
            assert_eq!(
                decode_varint(bytes),
                Ok(Some((value, bytes.len()))),
                "decoding {value}"
            );
        }
    }

    #[test]
    fn varint_round_trips_boundaries() {
        for value in [
            0,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0x1f_ffff,
            0x20_0000,
            0x0fff_ffff,
            0x1000_0000,
            i32::MAX,
            i32::MIN,
            -1,
            -128,
        ] {
            let bytes = encode(value);
            assert_eq!(decode_varint(&bytes), Ok(Some((value, bytes.len()))));
        }
    }

    #[test]
    fn varint_incomplete_and_too_long() {
        assert_eq!(decode_varint(&[]), Ok(None));
        assert_eq!(decode_varint(&[0x80]), Ok(None));
        assert_eq!(decode_varint(&[0xff, 0xff, 0xff, 0xff]), Ok(None));
        assert_eq!(
            decode_varint(&[0xff, 0xff, 0xff, 0xff, 0xff, 0x01]),
            Err(Error::VarIntTooLong)
        );
        // Trailing bytes after the value are not consumed.
        assert_eq!(decode_varint(&[0x05, 0xaa]), Ok(Some((5, 1))));
    }

    #[test]
    fn varint_byte_budget() {
        assert_eq!(decode_varint_max(&[0xff, 0x7f], 2), Ok(Some((16383, 2))));
        assert_eq!(
            decode_varint_max(&[0x80, 0x80, 0x01], 2),
            Err(Error::VarIntTooLong)
        );
    }

    #[test]
    fn varlong_round_trips() {
        let cases: &[(i64, usize)] = &[
            (0, 1),
            (127, 1),
            (128, 2),
            (i64::from(i32::MAX), 5),
            (i64::MAX, 9),
            (-1, 10),
            (i64::MIN, 10),
        ];
        for &(value, len) in cases {
            let bytes = encode_long(value);
            assert_eq!(bytes.len(), len, "length of {value}");
            assert_eq!(varlong_len(value), len);
            assert_eq!(decode_varlong(&bytes), Ok(Some((value, len))));
        }
        assert_eq!(decode_varlong(&[0xff; 11]), Err(Error::VarIntTooLong));
        assert_eq!(decode_varlong(&[0xff; 3]), Ok(None));
    }
}
