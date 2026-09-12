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

    @Comments({"Master switch for the AutoThread runtime.",
            "When false the server behaves exactly like upstream Crucible: single simulation thread,",
            "no region ownership, no parallel ticking. Metrics and profiling keep working.",
            "Only turn this off to compare against the baseline or to rule out AutoThread in a bug report."})
    public boolean gamma_autothread_enabled = true;

    @Comments({"Maximum number of threads used for world simulation (region ticking).",
            "0 = automatic: the runtime sizes it from the number of physical cores, never from SMT threads.",
            "Raising this above the physical core count usually lowers p99 tick time instead of raising throughput."})
    public int gamma_threads_simulation = 0;

    @Comments({"Maximum number of threads used for chunk disk IO (read, write, compression).",
            "0 = automatic."})
    public int gamma_threads_chunkIo = 0;

    @Comments({"Maximum number of threads used for chunk CPU work (NBT decode/encode, generation helpers).",
            "0 = automatic."})
    public int gamma_threads_chunkWorker = 0;

    @Comments({"Maximum number of threads used for general asynchronous tasks and plugin async work.",
            "0 = automatic."})
    public int gamma_threads_async = 0;

    @Comments({"Override of the detected physical core count.",
            "0 = automatic detection. Only set this when running in a container that reports the host topology."})
    public int gamma_threads_physicalCoresOverride = 0;

    @Comments({"Soft budget, in megabytes, for the caches the runtime is allowed to keep",
            "(chunk snapshots waiting to be written, learning profiles, pending region state).",
            "The runtime trims its caches when it goes over; it never hard-fails on this limit."})
    public int gamma_memory_budgetMb = 512;

    @Comment("Start collecting a profiling session as soon as the server finishes booting.")
    public boolean gamma_profiling_enabledAtStartup = false;

    @Comments({"How often, in seconds, repeated diagnostics are aggregated into a single summary line.",
            "A busy server can produce hundreds of thousands of identical conflict reports; without",
            "aggregation the log becomes the bottleneck. 0 disables aggregation (debug only)."})
    public int gamma_logging_aggregationSeconds = 60;

    @Comment("Log every AutoThread decision. Extremely verbose, for development only.")
    public boolean gamma_logging_verbose = false;

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

    /** Forces the configuration file to be read and rewritten; called early during boot. */
    public static void ensureLoaded() {
        // Touching the class is enough: the singleton constructor reads and rewrites the file.
        GammaEngine.LOGGER.debug("GammaEngine configuration loaded from {}", configs.CONFIG_FILE);
    }
}
