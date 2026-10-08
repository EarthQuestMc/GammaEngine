package io.github.gammaengine.metrics;

import io.github.gammaengine.autothread.AutoThreadRuntime;
import io.github.gammaengine.config.GammaConfig;
import net.minecraft.server.MinecraftServer;

import java.io.IOException;

/**
 * Records every tick of a measurement run to disk, for the phase 0 bench.
 *
 * <p>Off by default. When no recording runs, the cost per tick is one volatile read. A recording
 * starts from {@code /autothread record start}, or at server start when {@code gamma.bench.export}
 * is set in {@code gammaengine.yml}, and writes {@code ticks.csv}, {@code gc.csv} and
 * {@code summary.json}. With attribution ({@code gamma.bench.attribution}, or the
 * {@code attribution} argument of the command) it also charges every entity and tile entity tick to
 * its class, mod and chunk, and writes {@code mods.csv} and {@code chunks.csv}.
 *
 * <p>Thread invariant: start and stop run under the instance lock; the server thread only reads
 * {@link #active}, and a session stops receiving ticks before it is finished. Console commands and
 * the shutdown run on the server thread; a stop from another thread (RCON) waits for the server
 * thread to start a new tick before the session reads what that thread wrote.
 */
public final class BenchRecorder {
    private static final BenchRecorder INSTANCE = new BenchRecorder();
    private static final long STOP_WAIT_NANOS = 1_000_000_000L;

    private volatile Recording active;

    private BenchRecorder() {
    }

    public static BenchRecorder get() {
        return INSTANCE;
    }

    /** Starts a recording, with attribution when {@code gamma.bench.attribution} is set. */
    public String start(String name) {
        return start(name, GammaConfig.configs.gamma_bench_attribution);
    }

    /**
     * Starts a recording.
     *
     * @param attribution also charge every entity and tile entity tick to its class and chunk
     * @return {@code null} on success, otherwise the reason it did not start
     */
    public synchronized String start(String name, boolean attribution) {
        if (active != null) {
            return "A recording is already running in " + active.directory.getPath();
        }
        String safeName = name == null || name.isEmpty() ? "run" : name.replaceAll("[^A-Za-z0-9_-]", "_");
        try {
            Recording recording = new Recording(safeName, attribution);
            active = recording;
            if (recording.attribution != null) {
                TickAttribution.attach(recording.attribution);
            }
            return null;
        } catch (IOException | RuntimeException e) {
            return "Cannot start the recording: " + e;
        }
    }

    /**
     * Stops the running recording and writes its summary.
     *
     * @return the summary, or {@code null} when nothing was recording
     */
    public String stop() {
        Recording recording;
        synchronized (this) {
            recording = active;
            if (recording == null) {
                return null;
            }
            active = null;
            if (recording.attribution != null) {
                TickAttribution.detach();
            }
        }
        awaitServerThread();
        return recording.finish();
    }

    /**
     * From a thread other than the server thread, waits until the server thread starts a new tick,
     * at most one second: after that it no longer writes to the session it may have been holding.
     */
    private static void awaitServerThread() {
        if (AutoThreadRuntime.get().isMainThread()) {
            return;
        }
        MinecraftServer server = MinecraftServer.getServer();
        if (server == null || !server.isServerRunning()) {
            return;
        }
        int tick = server.getTickCounter();
        long deadline = System.nanoTime() + STOP_WAIT_NANOS;
        while (server.getTickCounter() == tick && System.nanoTime() < deadline) {
            try {
                Thread.sleep(5L);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
        }
    }

    public boolean isRecording() {
        return active != null;
    }

    /** True when the running recording attributes ticks to classes and chunks. */
    public boolean isAttributing() {
        Recording recording = active;
        return recording != null && recording.attribution != null;
    }

    /** Where the running recording writes, or {@code null}. */
    public String directory() {
        Recording recording = active;
        return recording == null ? null : recording.directory.getPath();
    }

    /** Server thread, end of every tick. */
    public void onTick(long durationNanos) {
        Recording recording = active;
        if (recording != null) {
            recording.onTick(durationNanos);
        }
    }
}
