package io.github.gammaengine.config;

import io.github.crucible.util.config.Comment;
import io.github.crucible.util.config.Comments;
import io.github.crucible.util.config.ConfigMode;
import io.github.crucible.util.config.InvalidConfigurationException;
import io.github.crucible.util.config.YamlConfig;
import io.github.gammaengine.GammaEngine;

import java.io.File;

/**
 * Administrator-facing configuration of the engine, stored in {@code gammaengine.yml}.
 *
 * <p>Every option is optional: the server must run with no file at all. Each optimisation gets its
 * own switch here. There is deliberately no way to declare a mod thread-safe or to list mods: the
 * engine handles any jar by itself, and an administrator guessing wrong would corrupt worlds.
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

    @Comments({"Record every tick and every garbage collection from server start to stop, for the bench:",
            "gammaengine/bench/startup-<date>/ticks.csv, gc.csv and summary.json.",
            "/autothread record start|stop does the same on demand. Costs nothing while off."})
    public boolean gamma_bench_export = false;

    @Comments({"Level 2 of the bench: every recording also times each entity and tile entity tick, charges it",
            "to its class, its mod or plugin and its chunk, and writes mods.csv and chunks.csv next to ticks.csv.",
            "/autothread record start <name> attribution does the same for one recording.",
            "Costs two nanoTime calls per ticked object while a recording runs, which inflates the MSPT:",
            "compare runs made with the same setting. Costs one field read per object while off."})
    public boolean gamma_bench_attribution = false;

    @Comments({"Allow the native (Rust) engine to be loaded when the library is present.",
            "When it is missing or fails to load, the server automatically falls back to the Java",
            "implementations, so turning this off only costs performance, never compatibility."})
    public boolean gamma_native_enabled = true;

    private GammaConfig() {
        CONFIG_FILE = new File("gammaengine.yml");
        CONFIG_MODE = ConfigMode.PATH_BY_UNDERSCORE;
        CONFIG_HEADER = new String[]{
                "GammaEngine configuration",
                "",
                "Every option is optional: the server runs with no configuration at all.",
                "Each optimisation has its own switch, so it can be turned off on its own.",
                "There is nothing to declare per mod or per plugin."
        };
        migrateLegacyFile();

        try {
            init();
            save(); // rewrite the file so new options appear after an update
        } catch (InvalidConfigurationException e) {
            GammaEngine.LOGGER.error("Failed to load gammaengine.yml, falling back to defaults", e);
        }
    }

    /** Carries over {@code GammaAutoThread.yml}, the name used by earlier builds; option paths did not change. */
    private void migrateLegacyFile() {
        File legacy = new File("GammaAutoThread.yml");
        if (legacy.isFile() && !CONFIG_FILE.exists() && !legacy.renameTo(CONFIG_FILE)) {
            GammaEngine.LOGGER.warn("Could not rename {} to {}, starting from defaults", legacy, CONFIG_FILE);
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
