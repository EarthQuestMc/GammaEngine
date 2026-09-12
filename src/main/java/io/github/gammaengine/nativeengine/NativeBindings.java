package io.github.gammaengine.nativeengine;

/**
 * Raw JNI declarations. The only caller allowed is {@link NativeEngine}.
 *
 * <p>Keeping every {@code native} method on one class means there is exactly one place to look
 * when a symbol fails to resolve, one place the ABI version is checked, and no way for a random
 * subsystem to reach the native library without going through the façade that owns the fallback.
 *
 * <p>Each method returns {@code null} (or 0 for the hash) when the native side failed, including
 * when it caught a Rust panic. Callers must treat that as "use the Java implementation", never as
 * an error to propagate.
 */
final class NativeBindings {
    private NativeBindings() {
    }

    static native int abiVersion();

    static native byte[] zlibCompress(byte[] input, int level);

    static native byte[] zlibDecompress(byte[] input, int sizeHint);

    static native long xxh64(byte[] input, long seed);
}
