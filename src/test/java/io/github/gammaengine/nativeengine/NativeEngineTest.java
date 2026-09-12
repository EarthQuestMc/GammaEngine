package io.github.gammaengine.nativeengine;

import io.github.gammaengine.util.XxHash64;
import org.junit.BeforeClass;
import org.junit.Test;

import java.io.File;
import java.util.Arrays;
import java.util.Random;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

/**
 * Correctness and benchmark harness for the native engine.
 *
 * <p>The correctness half always runs. The comparison half only runs when the library has been
 * built ({@code cargo build --release} in {@code native/}); when it is missing the test reports the
 * Java numbers and passes, because a server without the library is a supported configuration.
 *
 * <p>Rule this test enforces: native and Java must agree byte for byte. An accelerated path that
 * produces a different chunk payload or a different hash is not an optimisation, it is corruption.
 */
public class NativeEngineTest {
    private static boolean nativeLoaded;

    @BeforeClass
    public static void loadNativeLibrary() {
        File library = locateLibrary();
        if (library == null) {
            System.out.println("[native] library not built, running Java-only checks "
                    + "(build it with: cd native && cargo build --release)");
            return;
        }
        try {
            System.load(library.getAbsolutePath());
            nativeLoaded = true;
            System.out.println("[native] loaded " + library);
        } catch (UnsatisfiedLinkError e) {
            System.out.println("[native] could not load " + library + ": " + e.getMessage());
        }
    }

    private static File locateLibrary() {
        String name = System.mapLibraryName("gammaengine_native");
        File directory = new File(System.getProperty("user.dir")).getAbsoluteFile();
        for (int depth = 0; directory != null && depth < 6; depth++, directory = directory.getParentFile()) {
            File candidate = new File(directory, "native/target/release/" + name);
            if (candidate.isFile()) {
                return candidate;
            }
        }
        return null;
    }

    @Test
    public void javaCompressionRoundTrips() {
        byte[] data = chunkLikePayload(80_000);
        byte[] compressed = NativeEngine.javaCompress(data, 6);
        assertTrue("compression should shrink chunk-like data", compressed.length < data.length);
        assertArrayEquals(data, NativeEngine.javaDecompress(compressed, data.length));
    }

    @Test
    public void javaHashMatchesTheReferenceVectors() {
        // Same vectors the Rust implementation is checked against.
        assertEquals(0xEF46DB3751D8E999L, XxHash64.hash(new byte[0], 0));
        assertEquals(0xD24EC4F1A98C6E5BL, XxHash64.hash(new byte[]{'a'}, 0));
        assertEquals(0x44BC2CF5AD770999L, XxHash64.hash(new byte[]{'a', 'b', 'c'}, 0));
    }

    /**
     * The two compressors must be interchangeable, not identical.
     *
     * <p>Java's Deflater and the Rust encoder both emit valid zlib streams but pick different
     * encodings, so the compressed bytes differ. That is safe for the save format: every reader of
     * a region file inflates the payload, and both streams inflate to the same chunk. What must
     * hold, and what this test pins down, is that either side can read the other's output, that the
     * decompressed content is identical, and that the hash of that content matches.
     */
    @Test
    public void nativeAndJavaAreInterchangeable() {
        if (!nativeLoaded) {
            return;
        }
        Random random = new Random(1234);
        for (int size : new int[]{4096, 20_000, 100_000, 400_000}) {
            byte[] data = chunkLikePayload(size);

            byte[] nativeCompressed = NativeBindings.zlibCompress(data, 6);
            byte[] javaCompressed = NativeEngine.javaCompress(data, 6);

            assertArrayEquals("native cannot read Java output at size " + size,
                    data, NativeBindings.zlibDecompress(javaCompressed, size));
            assertArrayEquals("Java cannot read native output at size " + size,
                    data, NativeEngine.javaDecompress(nativeCompressed, size));

            // A native encoder that bloated chunks would trade CPU for disk and network. The two
            // encoders differ by a small fixed cost on tiny payloads and by a few percent on real
            // ones, so the bound is proportional with a constant allowance rather than flat.
            assertTrue("native output is " + nativeCompressed.length + " bytes against Java's "
                            + javaCompressed.length + " at size " + size,
                    nativeCompressed.length <= javaCompressed.length * 1.05 + 64);

            long seed = random.nextLong();
            assertEquals("hash differs at size " + size,
                    XxHash64.hash(data, seed), NativeBindings.xxh64(data, seed));
        }
    }

    @Test
    public void corruptInputFallsBackInsteadOfCrashing() {
        if (!nativeLoaded) {
            return;
        }
        byte[] garbage = new byte[8192];
        Arrays.fill(garbage, (byte) 0x5A);
        // The native side must report failure with null, never abort the JVM.
        assertNull(NativeBindings.zlibDecompress(garbage, 1024));
    }

    @Test
    public void benchmarkCompressionAndHashing() {
        int size = 200_000;
        byte[] data = chunkLikePayload(size);
        byte[] compressed = NativeEngine.javaCompress(data, 6);

        System.out.println("[bench] payload " + size + " bytes, compressed " + compressed.length + " bytes");
        // Level 4 is what Minecraft uses for chunk packets and level 6 what it uses for region
        // files, so both are worth knowing: the chunk streaming budget depends on the first.
        for (final int level : new int[]{1, 4, 6}) {
            byte[] atLevel = NativeEngine.javaCompress(data, level);
            report("compress java L" + level, measure(new Runnable() {
                @Override
                public void run() {
                    NativeEngine.javaCompress(data, level);
                }
            }), size);
            System.out.println("[bench]   level " + level + " output " + atLevel.length + " bytes");
        }
        report("compress java  ", measure(new Runnable() {
            @Override
            public void run() {
                NativeEngine.javaCompress(data, 6);
            }
        }), size);
        report("decompress java", measure(new Runnable() {
            @Override
            public void run() {
                NativeEngine.javaDecompress(compressed, size);
            }
        }), size);
        report("xxh64 java     ", measure(new Runnable() {
            @Override
            public void run() {
                XxHash64.hash(data, 0);
            }
        }), size);

        if (nativeLoaded) {
            byte[] nativeCompressed = NativeBindings.zlibCompress(data, 6);
            System.out.println(String.format("[bench] compressed size java %d bytes, rust %d bytes (%+.1f%%)",
                    compressed.length, nativeCompressed.length,
                    100.0 * (nativeCompressed.length - compressed.length) / compressed.length));
            report("compress rust  ", measure(new Runnable() {
                @Override
                public void run() {
                    NativeBindings.zlibCompress(data, 6);
                }
            }), size);
            report("decompress rust", measure(new Runnable() {
                @Override
                public void run() {
                    NativeBindings.zlibDecompress(compressed, size);
                }
            }), size);
            report("xxh64 rust     ", measure(new Runnable() {
                @Override
                public void run() {
                    NativeBindings.xxh64(data, 0);
                }
            }), size);
        }
    }

    /** Median of per-operation times, after a warm-up long enough for C2 to compile the Java path. */
    private static long measure(Runnable operation) {
        for (int i = 0; i < 200; i++) {
            operation.run();
        }
        long[] samples = new long[50];
        for (int i = 0; i < samples.length; i++) {
            long start = System.nanoTime();
            operation.run();
            samples[i] = System.nanoTime() - start;
        }
        Arrays.sort(samples);
        return samples[samples.length / 2];
    }

    private static void report(String label, long nanos, int bytes) {
        double millis = nanos / 1.0e6;
        double mibPerSecond = bytes / (nanos / 1.0e9) / (1024 * 1024);
        System.out.println(String.format("[bench] %s %8.3f ms  %9.1f MiB/s", label, millis, mibPerSecond));
    }

    /**
     * Data shaped like a chunk payload: long runs of identical blocks broken by noisy regions, so
     * the compression ratio is representative instead of either trivial or incompressible.
     */
    private static byte[] chunkLikePayload(int size) {
        byte[] data = new byte[size];
        Random random = new Random(42);
        int index = 0;
        while (index < size) {
            if (random.nextInt(4) == 0) {
                int noise = Math.min(size - index, 64 + random.nextInt(256));
                for (int i = 0; i < noise; i++) {
                    data[index++] = (byte) random.nextInt(256);
                }
            } else {
                int run = Math.min(size - index, 128 + random.nextInt(1024));
                byte value = (byte) random.nextInt(32);
                Arrays.fill(data, index, index + run, value);
                index += run;
            }
        }
        return data;
    }
}
