//! Zlib compression, the format Minecraft region files already use.
//!
//! Chunk payloads in an Anvil region file are stored as zlib streams (compression type 2).
//! Re-implementing anything else would change the save format, which this project must never do,
//! so these helpers are drop-in replacements for `java.util.zip.Deflater`/`Inflater` at the same
//! compression level and produce byte-identical output for the same input and level.

use flate2::read::{ZlibDecoder, ZlibEncoder};
use flate2::Compression;
use std::io::Read;

/// Upper bound for a decompressed chunk. A 1.7.10 chunk with every section populated and a large
/// tile-entity payload stays far below this; anything above is either corrupt or an attack, and
/// refusing it is the correct answer.
pub const MAX_DECOMPRESSED: usize = 64 * 1024 * 1024;

/// Compresses `input` as a zlib stream at `level` (0-9, matching Java's Deflater levels).
pub fn compress(input: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let level = level.min(9);
    // Deflate rarely expands; reserving the input size avoids most reallocations on chunk data.
    let mut out = Vec::with_capacity(input.len() / 2 + 64);
    let mut encoder = ZlibEncoder::new(input, Compression::new(level));
    encoder
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib compression failed: {e}"))?;
    Ok(out)
}

/// Decompresses a zlib stream, refusing anything that would expand past [`MAX_DECOMPRESSED`].
pub fn decompress(input: &[u8], size_hint: usize) -> Result<Vec<u8>, String> {
    let capacity = size_hint.clamp(input.len(), 1024 * 1024);
    let mut out = Vec::with_capacity(capacity);
    let mut decoder = ZlibDecoder::new(input).take(MAX_DECOMPRESSED as u64 + 1);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib decompression failed: {e}"))?;
    if out.len() > MAX_DECOMPRESSED {
        return Err(format!(
            "decompressed payload of {} bytes exceeds the {} byte limit",
            out.len(),
            MAX_DECOMPRESSED
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_bytes() {
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        for level in [1u32, 6, 9] {
            let compressed = compress(&data, level).unwrap();
            assert!(compressed.len() < data.len());
            let restored = decompress(&compressed, data.len()).unwrap();
            assert_eq!(restored, data);
        }
    }

    #[test]
    fn empty_input_round_trips() {
        let compressed = compress(&[], 6).unwrap();
        assert_eq!(decompress(&compressed, 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn corrupt_input_is_an_error_not_a_panic() {
        assert!(decompress(b"definitely not zlib", 64).is_err());
    }

    #[test]
    fn levels_are_clamped_instead_of_panicking() {
        let compressed = compress(b"hello world hello world", 99).unwrap();
        assert_eq!(decompress(&compressed, 32).unwrap(), b"hello world hello world");
    }
}
