package io.github.gammaengine.autothread;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.concurrent.ManagedPool;
import io.github.gammaengine.concurrent.ThreadPools;
import io.github.gammaengine.config.GammaConfig;
import io.github.gammaengine.platform.CpuTopology;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.TickStatistics;

/**
 * The AutoThread runtime: single owner of every threading decision the server makes.
 *
 * <p>The contract with the rest of the server is intentionally narrow. Patched Minecraft, Forge
 * and Bukkit code calls into this class at a handful of lifecycle points; it never asks the
 * runtime "may I parallelize this", it hands over work and the runtime decides. That keeps the
 * patch surface small enough to rebase on upstream Crucible, and it keeps the decision logic in
 * one place where it can be reasoned about.
 *
 * <p>Boot order, which matters because each step depends on the previous one:
 * <ol>
 *   <li>{@link #boot()} while the server is starting: configuration, CPU topology, thread pools.</li>
 *   <li>{@link #onServerStarted()} once worlds exist: subsystems that need a world.</li>
 *   <li>{@link #onTickStart()} / {@link #onTickEnd()} around every server tick.</li>
 *   <li>{@link #shutdown()} before the server saves and exits.</li>
 * </ol>
 *
 * <p>Thread ownership: the field {@link #mainThread} is the thread that ran {@link #boot()}, which
 * is the Minecraft server thread. Everything that has to answer "am I allowed to touch the world
 * right now" ultimately compares against it.
 */
public final class AutoThreadRuntime {
    private static final AutoThreadRuntime INSTANCE = new AutoThreadRuntime();

    private volatile boolean booted;
    private volatile boolean running;
    private volatile boolean stopped;
    private volatile Thread mainThread;
    private ThreadPools pools;
    private long tickStartNanos;
    private final java.util.List<Runnable> tickTasks = new java.util.concurrent.CopyOnWriteArrayList<Runnable>();

    private AutoThreadRuntime() {
    }

    public static AutoThreadRuntime get() {
        return INSTANCE;
    }

    /**
     * Initializes the runtime. Safe to call twice; the second call is ignored.
     *
     * <p>Called from the server thread early during boot, before worlds are loaded, so that pool
     * sizing and metrics are available to everything that comes after.
     */
    public synchronized void boot() {
        if (booted) {
            return;
        }
        mainThread = Thread.currentThread();
        GammaConfig.ensureLoaded();
        pools = ThreadPools.initialize();
        io.github.gammaengine.nativeengine.NativeEngine.get().load();
        booted = true;

        GammaEngine.LOGGER.info("{} AutoThread runtime ready ({}), simulation budget: {} worker(s) + main thread",
                GammaEngine.NAME, CpuTopology.get(), pools.regionTick().threads());
        if (!GammaConfig.configs.gamma_autothread_enabled) {
            GammaEngine.LOGGER.warn("AutoThread parallelism is disabled in GammaAutoThread.yml: "
                    + "the server will simulate on a single thread, like upstream Crucible.");
        }
    }

    /** Called once the server finished loading worlds and is about to accept players. */
    public void onServerStarted() {
        running = true;
        // Total startup, measured from JVM start rather than from the point vanilla starts counting,
        // because that is the number an operator actually waits through.
        try {
            long uptimeMillis = java.lang.management.ManagementFactory.getRuntimeMXBean().getUptime();
            GammaEngine.LOGGER.info("Startup complete in {} s from JVM start", String.format("%.2f", uptimeMillis / 1000.0));
            GammaProfiler.get().record("server.startup", uptimeMillis * 1_000_000L);
        } catch (Throwable ignored) {
            // A JVM without the runtime MX bean still boots; it just does not report the number.
        }
        if (GammaConfig.configs.gamma_profiling_enabledAtStartup) {
            GammaProfiler.get().startSession();
            GammaEngine.LOGGER.info("Profiling session started automatically (gamma.profiling.enabledAtStartup)");
        }
    }

    /**
     * Registers work to run on the server thread at the start of every tick.
     *
     * <p>Used by engine subsystems that need a heartbeat without adding another patch to
     * {@code MinecraftServer}. Tasks must be short: they run inside the tick they are measuring.
     */
    public void addTickTask(Runnable task) {
        tickTasks.add(task);
    }

    public void removeTickTask(Runnable task) {
        tickTasks.remove(task);
    }

    /** Called at the very beginning of {@code MinecraftServer.tick()}. */
    public void onTickStart() {
        tickStartNanos = System.nanoTime();
        if (!tickTasks.isEmpty()) {
            for (Runnable task : tickTasks) {
                try {
                    task.run();
                } catch (Throwable error) {
                    // An engine heartbeat must never be the reason a tick fails.
                    GammaEngine.LOGGER.error("Tick task failed", error);
                }
            }
        }
    }

    /** Called at the very end of {@code MinecraftServer.tick()}. */
    public void onTickEnd() {
        if (tickStartNanos == 0L) {
            return;
        }
        long duration = System.nanoTime() - tickStartNanos;
        tickStartNanos = 0L;
        GammaProfiler profiler = GammaProfiler.get();
        profiler.ticks().recordTick(duration);
        profiler.record("server.tick", duration);
    }

    /**
     * Called before the server saves and stops.
     *
     * <p>The server reaches this more than once on a normal stop: CraftBukkit stops the server and
     * the JVM shutdown hook stops it again. Shutting the pools down twice is harmless but logging
     * it twice makes an operator think something went wrong, so the second call returns silently.
     */
    public synchronized void shutdown() {
        if (!booted || stopped) {
            return;
        }
        stopped = true;
        running = false;
        if (GammaProfiler.get().sessionActive()) {
            String report = GammaProfiler.get().stopSession();
            if (report != null) {
                GammaProfiler.get().writeReport(report);
            }
        }
        if (pools != null) {
            pools.shutdown();
        }
        GammaEngine.LOGGER.info("{} AutoThread runtime stopped", GammaEngine.NAME);
    }

    public boolean isBooted() {
        return booted;
    }

    public boolean isRunning() {
        return running;
    }

    /** True when the calling thread is the Minecraft server thread. */
    public boolean isMainThread() {
        return Thread.currentThread() == mainThread;
    }

    public Thread mainThread() {
        return mainThread;
    }

    public ThreadPools pools() {
        return pools;
    }

    /** Whether the runtime is allowed to run world work in parallel at all. */
    public boolean parallelismEnabled() {
        return booted && GammaConfig.configs.gamma_autothread_enabled;
    }

    /** Multi-line status text used by {@code /autothread status}. */
    public String statusText() {
        StringBuilder out = new StringBuilder(512);
        TickStatistics ticks = GammaProfiler.get().ticks();
        out.append(GammaEngine.NAME).append(" AutoThread runtime\n");
        out.append("  State: ").append(booted ? (running ? "running" : "booted") : "not booted")
                .append(parallelismEnabled() ? "" : " (parallelism disabled in config)").append('\n');
        out.append("  CPU: ").append(CpuTopology.get()).append('\n');
        out.append(String.format("  TPS: %.2f / %.2f / %.2f (1m, 5m, 15m)%n",
                ticks.tps1m(), ticks.tps5m(), ticks.tps15m()));
        TickStatistics.Mspt mspt = ticks.mspt();
        if (mspt != null) {
            out.append("  MSPT: ").append(mspt).append('\n');
        }
        if (pools != null) {
            out.append("  Pools:\n");
            for (ManagedPool pool : pools.all()) {
                out.append("    ").append(pool).append('\n');
            }
        }
        return out.toString();
    }
}
