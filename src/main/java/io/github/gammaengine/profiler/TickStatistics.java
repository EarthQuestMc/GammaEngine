package io.github.gammaengine.profiler;

import java.util.Arrays;

/**
 * Rolling tick statistics: TPS over three windows and MSPT percentiles over the last minute.
 *
 * <p>Two different consumers need two different views. An administrator typing a command wants
 * "what is the server doing right now", which is a short rolling window; a benchmark run wants
 * "what did the server do during this run", which is the lifetime histogram held by the
 * {@link MetricsRegistry}. This class owns the first view and feeds the second.
 *
 * <p>Recording happens on the server thread only, so the ring buffer needs no synchronization on
 * write. Readers are commands and report tasks on other threads: they may observe a torn view of
 * the ring (one slot older than the rest), which cannot corrupt anything and cannot move a
 * percentile by more than one sample.
 */
public final class TickStatistics {
    /** One minute of ticks at the vanilla rate. */
    private static final int WINDOW_TICKS = 1200;
    private static final long NANOS_PER_SECOND = 1_000_000_000L;

    private final long[] durations = new long[WINDOW_TICKS];
    private volatile int recorded;
    private int cursor;

    private long lastTickEndNanos;
    private volatile double tps1m = 20.0;
    private volatile double tps5m = 20.0;
    private volatile double tps15m = 20.0;
    private volatile long totalTicks;

    /**
     * Records one completed server tick.
     *
     * @param durationNanos time spent inside the tick itself, excluding the sleep that follows it
     */
    public void recordTick(long durationNanos) {
        durations[cursor] = durationNanos;
        cursor = (cursor + 1) % WINDOW_TICKS;
        if (recorded < WINDOW_TICKS) {
            recorded++;
        }
        totalTicks++;

        long now = System.nanoTime();
        if (lastTickEndNanos != 0L) {
            long sinceLastTick = now - lastTickEndNanos;
            if (sinceLastTick > 0) {
                // Instant rate, then the same exponential smoothing Spigot uses so that the three
                // numbers keep the meaning administrators already know from /tps.
                double instantTps = Math.min(20.0, (double) NANOS_PER_SECOND / sinceLastTick);
                double elapsedSeconds = (double) sinceLastTick / NANOS_PER_SECOND;
                tps1m = smooth(tps1m, instantTps, elapsedSeconds, 60.0);
                tps5m = smooth(tps5m, instantTps, elapsedSeconds, 300.0);
                tps15m = smooth(tps15m, instantTps, elapsedSeconds, 900.0);
            }
        }
        lastTickEndNanos = now;
    }

    private static double smooth(double previous, double sample, double elapsedSeconds, double windowSeconds) {
        double weight = Math.exp(-elapsedSeconds / windowSeconds);
        return previous * weight + sample * (1.0 - weight);
    }

    public double tps1m() {
        return tps1m;
    }

    public double tps5m() {
        return tps5m;
    }

    public double tps15m() {
        return tps15m;
    }

    public long totalTicks() {
        return totalTicks;
    }

    /** Number of ticks currently held in the rolling window. */
    public int windowSize() {
        return recorded;
    }

    /**
     * MSPT percentiles over the rolling window, exact rather than bucketed because the window is
     * small enough to sort on demand.
     *
     * @return {@code null} when no tick has been recorded yet
     */
    public Mspt mspt() {
        int size = recorded;
        if (size == 0) {
            return null;
        }
        long[] copy = Arrays.copyOf(durations, size);
        Arrays.sort(copy);
        long sum = 0;
        for (long value : copy) {
            sum += value;
        }
        return new Mspt(size, (double) sum / size, pick(copy, 50), pick(copy, 95), pick(copy, 99),
                copy[copy.length - 1]);
    }

    private static long pick(long[] sorted, double percentile) {
        int index = (int) Math.ceil(percentile / 100.0 * sorted.length) - 1;
        if (index < 0) {
            index = 0;
        }
        if (index >= sorted.length) {
            index = sorted.length - 1;
        }
        return sorted[index];
    }

    /** MSPT summary over the rolling window, in milliseconds. */
    public static final class Mspt {
        private final int samples;
        private final double meanNanos;
        private final long p50Nanos;
        private final long p95Nanos;
        private final long p99Nanos;
        private final long maxNanos;

        Mspt(int samples, double meanNanos, long p50Nanos, long p95Nanos, long p99Nanos, long maxNanos) {
            this.samples = samples;
            this.meanNanos = meanNanos;
            this.p50Nanos = p50Nanos;
            this.p95Nanos = p95Nanos;
            this.p99Nanos = p99Nanos;
            this.maxNanos = maxNanos;
        }

        public int samples() {
            return samples;
        }

        public double mean() {
            return meanNanos / 1.0e6;
        }

        public double p50() {
            return p50Nanos / 1.0e6;
        }

        public double p95() {
            return p95Nanos / 1.0e6;
        }

        public double p99() {
            return p99Nanos / 1.0e6;
        }

        public double max() {
            return maxNanos / 1.0e6;
        }

        @Override
        public String toString() {
            return String.format("mean=%.2fms p50=%.2fms p95=%.2fms p99=%.2fms max=%.2fms",
                    mean(), p50(), p95(), p99(), max());
        }
    }
}
