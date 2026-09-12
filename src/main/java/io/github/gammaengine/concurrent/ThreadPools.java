package io.github.gammaengine.concurrent;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.config.GammaConfig;
import io.github.gammaengine.platform.CpuTopology;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * The engine's thread pools and the rules that size them.
 *
 * <p>There is deliberately no single "engine executor". Work with different shapes must not share
 * a queue: a two-second chunk compression sitting in front of a region tick would add two seconds
 * to that region's tick time. Each pool below has one job, one queue and one worker budget.
 *
 * <p>Sizing starts from physical cores, never from SMT threads. On the reference machine
 * (8 cores / 16 threads) that gives seven simulation workers plus the main server thread, which
 * saturates the eight cores with simulation while leaving the SMT siblings for IO, compression and
 * Netty rather than for more simulation threads that would only add contention.
 */
public final class ThreadPools {
    private static ThreadPools instance;

    private final ManagedPool regionTick;
    private final ManagedPool chunkIo;
    private final ManagedPool chunkWorker;
    private final ManagedPool async;
    private final ManagedPool nativeEngine;
    private final ManagedPool background;
    private final List<ManagedPool> all;
    private final int physicalCores;

    private ThreadPools(int physicalCores) {
        this.physicalCores = physicalCores;
        GammaConfig config = GammaConfig.configs;

        int simulation = resolve(config.gamma_threads_simulation, Math.max(1, physicalCores - 1));
        int io = resolve(config.gamma_threads_chunkIo, clamp(physicalCores / 2, 2, 8));
        int worker = resolve(config.gamma_threads_chunkWorker, clamp(physicalCores / 2, 2, 8));
        int asyncThreads = resolve(config.gamma_threads_async, clamp(physicalCores / 2, 2, 16));

        // Region ticking is the latency-critical pool: it runs at the same priority as the server
        // thread minus one, and it may grow up to the physical core count but never past it.
        this.regionTick = new ManagedPool("RegionTick", simulation, physicalCores, Thread.NORM_PRIORITY - 1);
        // Disk work blocks on IO far more than it burns CPU, so it gets lower priority and may
        // exceed the core count slightly without hurting the simulation.
        this.chunkIo = new ManagedPool("ChunkIO", io, Math.max(io, physicalCores), Thread.NORM_PRIORITY - 2);
        this.chunkWorker = new ManagedPool("ChunkWorker", worker, Math.max(worker, physicalCores), Thread.NORM_PRIORITY - 2);
        this.async = new ManagedPool("Async", asyncThreads, Math.max(asyncThreads, physicalCores * 2), Thread.NORM_PRIORITY - 2);
        this.nativeEngine = new ManagedPool("Native", clamp(physicalCores / 4, 1, 4), physicalCores, Thread.NORM_PRIORITY - 2);
        // Maintenance, metric aggregation, report writing: never urgent, never allowed to steal a
        // core from the simulation.
        this.background = new ManagedPool("Background", 1, 2, Thread.MIN_PRIORITY + 1);

        List<ManagedPool> pools = new ArrayList<ManagedPool>();
        pools.add(regionTick);
        pools.add(chunkIo);
        pools.add(chunkWorker);
        pools.add(async);
        pools.add(nativeEngine);
        pools.add(background);
        this.all = Collections.unmodifiableList(pools);
    }

    private static int resolve(int configured, int automatic) {
        return configured > 0 ? configured : automatic;
    }

    private static int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(max, value));
    }

    /** Creates the pools on first call. Called once while the server boots. */
    public static synchronized ThreadPools initialize() {
        if (instance == null) {
            int override = GammaConfig.configs.gamma_threads_physicalCoresOverride;
            int cores = override > 0 ? override : CpuTopology.get().physicalCores();
            instance = new ThreadPools(cores);
            GammaEngine.LOGGER.info("CPU topology: {}", CpuTopology.get());
            GammaEngine.LOGGER.info("Thread pools: region={} chunkIo={} chunkWorker={} async={} native={}",
                    instance.regionTick.threads(), instance.chunkIo.threads(), instance.chunkWorker.threads(),
                    instance.async.threads(), instance.nativeEngine.threads());
        }
        return instance;
    }

    /** The pools, or {@code null} when the engine has not booted yet. */
    public static ThreadPools get() {
        return instance;
    }

    public ManagedPool regionTick() {
        return regionTick;
    }

    public ManagedPool chunkIo() {
        return chunkIo;
    }

    public ManagedPool chunkWorker() {
        return chunkWorker;
    }

    public ManagedPool async() {
        return async;
    }

    public ManagedPool nativeEngine() {
        return nativeEngine;
    }

    public ManagedPool background() {
        return background;
    }

    public List<ManagedPool> all() {
        return all;
    }

    /** Physical cores the sizing was based on. */
    public int physicalCores() {
        return physicalCores;
    }

    /** Stops every pool, letting queued world work finish first. */
    public void shutdown() {
        for (ManagedPool pool : all) {
            pool.shutdown(pool == chunkIo ? 30_000L : 10_000L);
        }
    }
}
