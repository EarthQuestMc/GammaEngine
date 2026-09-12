package io.github.gammaengine.nativeengine;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.config.GammaConfig;

import java.io.File;

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
            status = "disabled in GammaEngine.yml";
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
            available = true;
            status = "loaded from java.library.path (" + name + ")";
            GammaEngine.LOGGER.info("Native engine loaded: {}", status);
            return true;
        } catch (UnsatisfiedLinkError | SecurityException e) {
            return false;
        }
    }

    private boolean tryLoadFile(File file) {
        try {
            System.load(file.getAbsolutePath());
            available = true;
            status = "loaded from " + file.getAbsolutePath();
            GammaEngine.LOGGER.info("Native engine loaded: {}", status);
            return true;
        } catch (UnsatisfiedLinkError | SecurityException e) {
            GammaEngine.LOGGER.info("Native engine at {} could not be loaded ({}), using Java implementations",
                    file, e.getMessage());
            return false;
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
