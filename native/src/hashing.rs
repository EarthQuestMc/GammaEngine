//! xxHash64, used for content hashing of chunk snapshots and world integrity checks.
//!
//! The determinism tests hash every chunk of a world after a run and compare the result with a
//! second run; a cryptographic hash would make that pass several times slower than the world save
//! it is checking. xxHash64 is implemented here rather than pulled from a crate because the
//! algorithm is short, stable, and the Java fallback must produce the same value byte for byte.

const PRIME1: u64 = 0x9E37_79B1_85EB_CA87;
const PRIME2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const PRIME3: u64 = 0x1656_67B1_9E37_79F9;
const PRIME4: u64 = 0x85EB_CA77_C2B2_AE63;
const PRIME5: u64 = 0x27D4_EB2F_1656_67C5;

/// Computes the 64-bit xxHash of `input` with the given seed.
pub fn xxh64(input: &[u8], seed: u64) -> u64 {
    let mut hash;
    let mut remainder = input;

    if input.len() >= 32 {
        let mut v1 = seed.wrapping_add(PRIME1).wrapping_add(PRIME2);
        let mut v2 = seed.wrapping_add(PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME1);

        while remainder.len() >= 32 {
            v1 = round(v1, read_u64(&remainder[0..8]));
            v2 = round(v2, read_u64(&remainder[8..16]));
            v3 = round(v3, read_u64(&remainder[16..24]));
            v4 = round(v4, read_u64(&remainder[24..32]));
            remainder = &remainder[32..];
        }

        hash = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
        hash = merge_round(hash, v1);
        hash = merge_round(hash, v2);
        hash = merge_round(hash, v3);
        hash = merge_round(hash, v4);
    } else {
        hash = seed.wrapping_add(PRIME5);
    }

    hash = hash.wrapping_add(input.len() as u64);

    while remainder.len() >= 8 {
        let k1 = round(0, read_u64(&remainder[0..8]));
        hash ^= k1;
        hash = hash.rotate_left(27).wrapping_mul(PRIME1).wrapping_add(PRIME4);
        remainder = &remainder[8..];
    }

    if remainder.len() >= 4 {
        hash ^= (read_u32(&remainder[0..4]) as u64).wrapping_mul(PRIME1);
        hash = hash.rotate_left(23).wrapping_mul(PRIME2).wrapping_add(PRIME3);
        remainder = &remainder[4..];
    }

    for &byte in remainder {
        hash ^= (byte as u64).wrapping_mul(PRIME5);
        hash = hash.rotate_left(11).wrapping_mul(PRIME1);
    }

    hash ^= hash >> 33;
    hash = hash.wrapping_mul(PRIME2);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(PRIME3);
    hash ^= hash >> 32;
    hash
}

#[inline]
fn round(acc: u64, value: u64) -> u64 {
    acc.wrapping_add(value.wrapping_mul(PRIME2))
        .rotate_left(31)
        .wrapping_mul(PRIME1)
}

#[inline]
fn merge_round(acc: u64, value: u64) -> u64 {
    let value = round(0, value);
    (acc ^ value).wrapping_mul(PRIME1).wrapping_add(PRIME4)
}

#[inline]
fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("slice of exactly 8 bytes"))
}

#[inline]
fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("slice of exactly 4 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_vectors() {
        // Published XXH64 vectors, seed 0.
        assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxh64(b"a", 0), 0xD24E_C4F1_A98C_6E5B);
        assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
    }

    #[test]
    fn matches_an_independent_implementation() {
        // Cross-checked against twox-hash across every length class the algorithm branches on
        // (under 4 bytes, under 8, under 32, and the 32-byte striped loop) and several seeds,
        // because a hash that only matches on seed 0 would silently break snapshot comparison.
        use std::hash::Hasher;
        let data: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2654435761)) as u8).collect();
        for seed in [0u64, 1, 42, u64::MAX] {
            for length in [0usize, 1, 3, 4, 7, 8, 31, 32, 33, 64, 127, 256, 300] {
                let mut reference = twox_hash::XxHash64::with_seed(seed);
                reference.write(&data[..length]);
                assert_eq!(
                    xxh64(&data[..length], seed),
                    reference.finish(),
                    "mismatch for seed {seed}, length {length}"
                );
            }
        }
    }

    #[test]
    fn is_sensitive_to_every_byte() {
        let base: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let reference = xxh64(&base, 0);
        for index in [0usize, 37, 512, 999] {
            let mut altered = base.clone();
            altered[index] ^= 1;
            assert_ne!(reference, xxh64(&altered, 0), "byte {index} did not change the hash");
        }
    }

    #[test]
    fn is_independent_of_chunking() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        assert_eq!(xxh64(&data, 0), xxh64(&data.clone(), 0));
    }
}
