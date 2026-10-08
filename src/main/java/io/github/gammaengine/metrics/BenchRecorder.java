package io.github.gammaengine.metrics;

import java.io.IOException;

/**
 * Records every tick of a measurement run to disk, for the phase 0 bench.
 *
 * <p>Off by default. When no recording runs, the cost per tick is one volatile read. A recording
 * starts from {@code /autothread record start}, or at server start when {@code gamma.bench.export}
 * is set in {@code gammaengine.yml}, and writes {@code ticks.csv}, {@code gc.csv} and
 * {@code summary.json}.
 *
 * <p>Thread invariant: start and stop run under the instance lock; the server thread only reads
 * {@link #active}, and a session stops receiving ticks before it is finished.
 */
public final class BenchRecorder {
    private static final BenchRecorder INSTANCE = new BenchRecorder();

    private volatile Recording active;

    private BenchRecorder() {
    }

    public static BenchRecorder get() {
        return INSTANCE;
    }

    /**
     * Starts a recording.
     *
     * @return {@code null} on success, otherwise the reason it did not start
     */
    public synchronized String start(String name) {
        if (active != null) {
            return "A recording is already running in " + active.directory.getPath();
        }
        String safeName = name == null || name.isEmpty() ? "run" : name.replaceAll("[^A-Za-z0-9_-]", "_");
        try {
            active = new Recording(safeName);
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
    public synchronized String stop() {
        Recording recording = active;
        if (recording == null) {
            return null;
        }
        active = null;
        return recording.finish();
    }

    public boolean isRecording() {
        return active != null;
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
