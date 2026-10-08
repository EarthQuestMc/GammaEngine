package io.github.gammaengine.config;

import io.github.crucible.util.config.Comment;
import io.github.crucible.util.config.Comments;
import io.github.crucible.util.config.ConfigMode;
import io.github.crucible.util.config.InvalidConfigurationException;
import io.github.crucible.util.config.YamlConfig;
import io.github.gammaengine.GammaEngine;

import java.io.File;

/**
 * Administrator-facing configuration of the AutoThread runtime, stored in {@code GammaAutoThread.yml}.
 *
 * <p>The design rule of this file is that it contains resource limits and diagnostics switches,
 * and nothing else. There is deliberately no way to declare a mod thread-safe, to pin a plugin to
 * a thread, or to pick a threading mode: those decisions belong to the runtime, which observes
 * what the code actually does, and an administrator guessing them wrong would corrupt worlds.
 *
 * <p>Zero means "decide automatically" for every sizing option.
 */
public class GammaConfig extends YamlConfig {
    public static final GammaConfig configs = new GammaConfig();

    @Comments({"Deflate level used for chunk packets sent to clients, 1 to 9.",
            "Measured on chunk-shaped data: level 1 is 70% faster than level 4 and produces 3.7% more",
            "bytes, level 6 costs 39% more CPU than level 4 and saves 0.08% of the bytes.",
            "4 is the best trade today. Lower it to 1 if the CPU is the bottleneck and bandwidth is free;",
            "raise it once chunk payloads are cached and shared between players, because compression then",
            "happens once per chunk instead of once per player."})
    public int gamma_network_chunkCompressionLevel = 4;

    @Comment("Start collecting a profiling session as soon as the server finishes booting.")
    public boolean gamma_profiling_enabledAtStartup = false;

    @Comments({"Allow the native (Rust) engine to be loaded when the library is present.",
            "When it is missing or fails to load, the server automatically falls back to the Java",
            "implementations, so turning this off only costs performance, never compatibility."})
    public boolean gamma_native_enabled = true;

    private GammaConfig() {
        CONFIG_FILE = new File("GammaAutoThread.yml");
        CONFIG_MODE = ConfigMode.PATH_BY_UNDERSCORE;
        CONFIG_HEADER = new String[]{
                "GammaEngine AutoThread configuration",
                "",
                "This file only contains resource limits and diagnostics. The AutoThread runtime decides",
                "by itself which mod, plugin, entity or tile entity code can run in parallel, by observing",
                "what that code actually touches at runtime. There is nothing to declare here per mod."
        };

        try {
            init();
            save(); // rewrite the file so new options appear after an update
        } catch (InvalidConfigurationException e) {
            GammaEngine.LOGGER.error("Failed to load GammaAutoThread.yml, falling back to defaults", e);
        }
    }

    /**
     * Chunk packet deflate level, clamped to what {@link java.util.zip.Deflater} accepts.
     *
     * <p>Clamping rather than validating on load: a bad value in the file must not stop a server
     * from booting, and a chunk packet must never be the thing that throws.
     */
    public static int chunkCompressionLevel() {
        int level = configs.gamma_network_chunkCompressionLevel;
        return level < 1 ? 1 : (level > 9 ? 9 : level);
    }

    /** Forces the configuration file to be read and rewritten; called early during boot. */
    public static void ensureLoaded() {
        // Touching the class is enough: the singleton constructor reads and rewrites the file.
        GammaEngine.LOGGER.debug("GammaEngine configuration loaded from {}", configs.CONFIG_FILE);
    }
}
