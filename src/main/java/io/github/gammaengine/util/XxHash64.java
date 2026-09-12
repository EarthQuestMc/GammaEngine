package io.github.gammaengine.util;

/**
 * XXH64, the Java side of the hash used for chunk snapshots and world integrity checks.
 *
 * <p>This must produce exactly the same value as {@code native/src/hashing.rs}. A determinism run
 * that hashes a world with the native library present and compares it with a run that fell back to
 * Java would otherwise report a corruption that does not exist, which is worse than no check at
 * all. The two implementations are kept in sync by a test that hashes the same inputs through both.
 */
public final class XxHash64 {
    private static final long PRIME1 = 0x9E3779B185EBCA87L;
    private static final long PRIME2 = 0xC2B2AE3D27D4EB4FL;
    private static final long PRIME3 = 0x165667B19E3779F9L;
    private static final long PRIME4 = 0x85EBCA77C2B2AE63L;
    private static final long PRIME5 = 0x27D4EB2F165667C5L;

    private XxHash64() {
    }

    public static long hash(byte[] input, long seed) {
        return hash(input, 0, input.length, seed);
    }

    public static long hash(byte[] input, int offset, int length, long seed) {
        int index = offset;
        final int end = offset + length;
        long hash;

        if (length >= 32) {
            long v1 = seed + PRIME1 + PRIME2;
            long v2 = seed + PRIME2;
            long v3 = seed;
            long v4 = seed - PRIME1;

            final int limit = end - 32;
            while (index <= limit) {
                v1 = round(v1, readLong(input, index));
                v2 = round(v2, readLong(input, index + 8));
                v3 = round(v3, readLong(input, index + 16));
                v4 = round(v4, readLong(input, index + 24));
                index += 32;
            }

            hash = Long.rotateLeft(v1, 1) + Long.rotateLeft(v2, 7)
                    + Long.rotateLeft(v3, 12) + Long.rotateLeft(v4, 18);
            hash = mergeRound(hash, v1);
            hash = mergeRound(hash, v2);
            hash = mergeRound(hash, v3);
            hash = mergeRound(hash, v4);
        } else {
            hash = seed + PRIME5;
        }

        hash += length;

        while (end - index >= 8) {
            hash ^= round(0, readLong(input, index));
            hash = Long.rotateLeft(hash, 27) * PRIME1 + PRIME4;
            index += 8;
        }

        if (end - index >= 4) {
            hash ^= (readInt(input, index) & 0xFFFFFFFFL) * PRIME1;
            hash = Long.rotateLeft(hash, 23) * PRIME2 + PRIME3;
            index += 4;
        }

        while (index < end) {
            hash ^= (input[index] & 0xFFL) * PRIME5;
            hash = Long.rotateLeft(hash, 11) * PRIME1;
            index++;
        }

        hash ^= hash >>> 33;
        hash *= PRIME2;
        hash ^= hash >>> 29;
        hash *= PRIME3;
        hash ^= hash >>> 32;
        return hash;
    }

    private static long round(long accumulator, long value) {
        return Long.rotateLeft(accumulator + value * PRIME2, 31) * PRIME1;
    }

    private static long mergeRound(long accumulator, long value) {
        return (accumulator ^ round(0, value)) * PRIME1 + PRIME4;
    }

    private static long readLong(byte[] data, int index) {
        return (data[index] & 0xFFL)
                | (data[index + 1] & 0xFFL) << 8
                | (data[index + 2] & 0xFFL) << 16
                | (data[index + 3] & 0xFFL) << 24
                | (data[index + 4] & 0xFFL) << 32
                | (data[index + 5] & 0xFFL) << 40
                | (data[index + 6] & 0xFFL) << 48
                | (data[index + 7] & 0xFFL) << 56;
    }

    private static int readInt(byte[] data, int index) {
        return (data[index] & 0xFF)
                | (data[index + 1] & 0xFF) << 8
                | (data[index + 2] & 0xFF) << 16
                | (data[index + 3] & 0xFF) << 24;
    }
}
