package io.github.gammaengine.nativeengine;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.config.GammaConfig;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.util.XxHash64;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.util.zip.DataFormatException;
import java.util.zip.Deflater;
import java.util.zip.Inflater;

/**
 * Single façade between Java and the optional native (Rust) library.
 *
 * <p>Every JNI entry point in the engine goes through this class. Scattering {@code native}
 * declarations across the codebase would make the fallback path impossible to reason about: here
 * there is exactly one place that knows whether the library loaded, one place that counts JNI
 * calls, and one place that decides to use Java instead.
 *
 * <p>The library is always optional. A server with no {@code libgammaengine_native.so} must boot,
 * run and produce identical results, only slower on the paths the native code would accelerate.
 * Loading failures are logged once at INFO level, never thrown.
 */
public final class NativeEngine {
    /** Must match {@code ABI_VERSION} in {@code native/src/lib.rs}. */
    public static final int ABI_VERSION = 1;

    /**
     * Below this size the JNI transition and the two array copies cost more than the work saved,
     * measured on the reference machine. Chunk payloads are comfortably above it; anything smaller
     * stays in Java.
     */
    private static final int MIN_NATIVE_BYTES = 4096;

    private static final NativeEngine INSTANCE = new NativeEngine();

    private volatile boolean available;
    private volatile String status = "not loaded";

    private NativeEngine() {
    }

    public static NativeEngine get() {
        return INSTANCE;
    }

    /**
     * Attempts to load the native library. Called once during boot.
     *
     * <p>Search order: the {@code gammaengine.nativeLibrary} system property (absolute path, used by
     * tests and by packagers), then the standard {@code java.library.path}, then
     * {@code gammaengine/native} next to the server jar.
     */
    public synchronized void load() {
        if (available) {
            return;
        }
        if (!GammaConfig.configs.gamma_native_enabled) {
            status = "disabled in GammaAutoThread.yml";
            GammaEngine.LOGGER.info("Native engine disabled by configuration, using Java implementations");
            return;
        }

        String explicit = System.getProperty("gammaengine.nativeLibrary");
        if (explicit != null && tryLoadFile(new File(explicit))) {
            return;
        }
        if (tryLoadLibrary("gammaengine_native")) {
            return;
        }
        File local = new File("gammaengine/native", System.mapLibraryName("gammaengine_native"));
        if (local.isFile() && tryLoadFile(local)) {
            return;
        }

        status = "not present, using Java implementations";
        GammaEngine.LOGGER.info("Native engine not found on this system; every accelerated path falls back to Java");
    }

    private boolean tryLoadLibrary(String name) {
        try {
            System.loadLibrary(name);
            return accept("java.library.path (" + name + ")");
        } catch (UnsatisfiedLinkError | SecurityException e) {
            return false;
        }
    }

    private boolean tryLoadFile(File file) {
        try {
            System.load(file.getAbsolutePath());
            return accept(file.getAbsolutePath());
        } catch (UnsatisfiedLinkError | SecurityException e) {
            GammaEngine.LOGGER.info("Native engine at {} could not be loaded ({}), using Java implementations",
                    file, e.getMessage());
            return false;
        }
    }

    /**
     * Validates the ABI of a library that just loaded.
     *
     * <p>A stale library from a previous build links fine and then misbehaves in ways that look
     * like world corruption, so a version mismatch disables native acceleration outright.
     */
    private boolean accept(String origin) {
        int version;
        try {
            version = NativeBindings.abiVersion();
        } catch (UnsatisfiedLinkError e) {
            status = "loaded from " + origin + " but does not export the expected symbols";
            GammaEngine.LOGGER.warn("Native engine rejected: {}", status);
            return false;
        }
        if (version != ABI_VERSION) {
            status = "ABI " + version + " from " + origin + ", this server needs ABI " + ABI_VERSION;
            GammaEngine.LOGGER.warn("Native engine rejected: {}. Rebuild it with 'cargo build --release' "
                    + "in native/.", status);
            return false;
        }
        available = true;
        status = "ABI " + version + " loaded from " + origin;
        GammaEngine.LOGGER.info("Native engine loaded: {}", status);
        return true;
    }

    /**
     * Compresses a chunk payload as a zlib stream, exactly like {@link Deflater} at the same level.
     *
     * <p>Small buffers stay in Java: a JNI transition plus two array copies costs more than
     * deflating a few kilobytes, and the region-file writer calls this once per chunk, not once per
     * block.
     */
    public byte[] compress(byte[] data, int level) {
        if (available && data.length >= MIN_NATIVE_BYTES) {
            long start = System.nanoTime();
            byte[] result = NativeBindings.zlibCompress(data, level);
            if (result != null) {
                recordNativeCall("compress", start, data.length);
                return result;
            }
            // The native side refused or panicked; the Java path below is always correct.
            GammaProfiler.get().count("native.fallback.compress");
        }
        long start = System.nanoTime();
        byte[] result = javaCompress(data, level);
        GammaProfiler.get().record("native.compress.java", System.nanoTime() - start);
        return result;
    }

    /** Decompresses a zlib stream produced by {@link #compress}. */
    public byte[] decompress(byte[] data, int sizeHint) {
        if (available && data.length >= MIN_NATIVE_BYTES) {
            long start = System.nanoTime();
            byte[] result = NativeBindings.zlibDecompress(data, sizeHint);
            if (result != null) {
                recordNativeCall("decompress", start, data.length);
                return result;
            }
            GammaProfiler.get().count("native.fallback.decompress");
        }
        long start = System.nanoTime();
        byte[] result = javaDecompress(data, sizeHint);
        GammaProfiler.get().record("native.decompress.java", System.nanoTime() - start);
        return result;
    }

    /**
     * XXH64 of a buffer. Native and Java produce the same value by construction, so a world hash
     * computed on one path can be compared with a hash computed on the other.
     */
    public long hash(byte[] data, long seed) {
        if (available && data.length >= MIN_NATIVE_BYTES) {
            long start = System.nanoTime();
            long result = NativeBindings.xxh64(data, seed);
            // 0 is a legal hash value, so it cannot signal failure; the native side only returns it
            // on panic, and recomputing in Java costs one hash on an event that should never happen.
            if (result != 0L) {
                recordNativeCall("hash", start, data.length);
                return result;
            }
        }
        return XxHash64.hash(data, seed);
    }

    private void recordNativeCall(String operation, long startNanos, int bytes) {
        GammaProfiler profiler = GammaProfiler.get();
        profiler.record("native." + operation, System.nanoTime() - startNanos);
        profiler.registry().counter("native.jni.calls").increment();
        profiler.registry().counter("native.jni.bytes").add(bytes);
    }

    static byte[] javaCompress(byte[] data, int level) {
        Deflater deflater = new Deflater(Math.max(0, Math.min(9, level)));
        try {
            deflater.setInput(data);
            deflater.finish();
            ByteArrayOutputStream out = new ByteArrayOutputStream(Math.max(64, data.length / 2));
            byte[] buffer = new byte[8192];
            while (!deflater.finished()) {
                int written = deflater.deflate(buffer);
                out.write(buffer, 0, written);
            }
            return out.toByteArray();
        } finally {
            deflater.end();
        }
    }

    static byte[] javaDecompress(byte[] data, int sizeHint) {
        Inflater inflater = new Inflater();
        try {
            inflater.setInput(data);
            ByteArrayOutputStream out = new ByteArrayOutputStream(Math.max(1024, sizeHint));
            byte[] buffer = new byte[8192];
            while (!inflater.finished()) {
                int written = inflater.inflate(buffer);
                if (written == 0 && (inflater.needsInput() || inflater.needsDictionary())) {
                    break; // truncated stream: report what we have rather than spinning forever
                }
                out.write(buffer, 0, written);
            }
            return out.toByteArray();
        } catch (DataFormatException e) {
            throw new IllegalArgumentException("corrupt zlib stream", e);
        } finally {
            inflater.end();
        }
    }

    /** True when native acceleration is usable. Callers must always have a Java path too. */
    public boolean isAvailable() {
        return available;
    }

    public String statusText() {
        return available ? "available: " + status : "unavailable: " + status;
    }
}
