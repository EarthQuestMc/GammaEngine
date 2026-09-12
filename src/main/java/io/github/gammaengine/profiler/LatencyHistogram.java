package io.github.gammaengine.profiler;

import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicLongArray;

/**
 * A compact, thread-safe latency histogram used everywhere GammaEngine reports p50/p95/p99.
 *
 * <p>Averages hide exactly the behaviour a multithreaded server has to control: a tick loop whose
 * mean is 20 ms but whose p99 is 180 ms is a server that stutters four times a minute. The
 * histogram keeps percentiles cheap enough to record on every tick, on every region tick and on
 * every chunk save.
 *
 * <p>Layout is the classic HdrHistogram shape, shrunk to one array: values below 64 get their own
 * bucket, and above that each power of two is split into 32 buckets, which bounds the relative
 * error of any reported percentile to about 3%. Recording is a single atomic increment; it never
 * allocates and never blocks, so a worker thread can record while the profiler reads.
 */
public final class LatencyHistogram {
    private static final int SUB_BITS = 5;
    private static final int SUB_COUNT = 1 << SUB_BITS;
    private static final int LINEAR_LIMIT = SUB_COUNT * 2;
    /** Enough for values up to 2^62 ns, i.e. far beyond any realistic duration. */
    private static final int BUCKET_COUNT = 60 * SUB_COUNT;

    private final String name;
    private final AtomicLongArray buckets = new AtomicLongArray(BUCKET_COUNT);
    private final AtomicLong count = new AtomicLong();
    private final AtomicLong total = new AtomicLong();
    private final AtomicLong max = new AtomicLong();

    public LatencyHistogram(String name) {
        this.name = name;
    }

    public String name() {
        return name;
    }

    /**
     * Records one observation, in nanoseconds. Negative values are clamped to zero: a duration
     * computed from {@link System#nanoTime()} can come out negative when the measured code was
     * migrated between cores on some platforms, and losing that sample is better than corrupting
     * the histogram.
     */
    public void record(long nanos) {
        if (nanos < 0) {
            nanos = 0;
        }
        buckets.incrementAndGet(indexOf(nanos));
        count.incrementAndGet();
        total.addAndGet(nanos);
        long currentMax = max.get();
        while (nanos > currentMax && !max.compareAndSet(currentMax, nanos)) {
            currentMax = max.get();
        }
    }

    public long count() {
        return count.get();
    }

    public long totalNanos() {
        return total.get();
    }

    public long maxNanos() {
        return max.get();
    }

    public double meanNanos() {
        long n = count.get();
        return n == 0 ? 0.0 : (double) total.get() / (double) n;
    }

    /**
     * Estimated percentile in nanoseconds, {@code percentile} being expressed between 0 and 100.
     * The returned value is the midpoint of the bucket that contains the requested rank, so it is
     * accurate to the histogram resolution and never to the nanosecond.
     */
    public long percentileNanos(double percentile) {
        long n = count.get();
        if (n == 0) {
            return 0;
        }
        double clamped = Math.max(0.0, Math.min(100.0, percentile));
        long rank = (long) Math.ceil(clamped / 100.0 * n);
        if (rank < 1) {
            rank = 1;
        }
        long seen = 0;
        for (int i = 0; i < BUCKET_COUNT; i++) {
            seen += buckets.get(i);
            if (seen >= rank) {
                return midpointOf(i);
            }
        }
        return max.get();
    }

    /** Clears every counter. Used when a profiling session starts. */
    public void reset() {
        for (int i = 0; i < BUCKET_COUNT; i++) {
            buckets.set(i, 0);
        }
        count.set(0);
        total.set(0);
        max.set(0);
    }

    /** Immutable view of the current state, safe to format or serialize. */
    public Snapshot snapshot() {
        return new Snapshot(name, count(), meanNanos(), percentileNanos(50), percentileNanos(95),
                percentileNanos(99), maxNanos(), totalNanos());
    }

    static int indexOf(long value) {
        if (value < LINEAR_LIMIT) {
            return (int) value;
        }
        int magnitude = 63 - Long.numberOfLeadingZeros(value);
        int shift = magnitude - SUB_BITS;
        int sub = (int) ((value >>> shift) & (SUB_COUNT - 1));
        int index = (shift + 1) * SUB_COUNT + sub;
        return index >= BUCKET_COUNT ? BUCKET_COUNT - 1 : index;
    }

    static long lowerBoundOf(int index) {
        if (index < LINEAR_LIMIT) {
            return index;
        }
        int shift = index / SUB_COUNT - 1;
        int sub = index % SUB_COUNT;
        return ((long) (sub + SUB_COUNT)) << shift;
    }

    static long midpointOf(int index) {
        if (index < LINEAR_LIMIT) {
            return index;
        }
        int shift = index / SUB_COUNT - 1;
        return lowerBoundOf(index) + (1L << shift) / 2;
    }

    /** Point-in-time copy of a histogram, with every duration already converted to milliseconds. */
    public static final class Snapshot {
        private final String name;
        private final long count;
        private final double meanNanos;
        private final long p50Nanos;
        private final long p95Nanos;
        private final long p99Nanos;
        private final long maxNanos;
        private final long totalNanos;

        Snapshot(String name, long count, double meanNanos, long p50Nanos, long p95Nanos, long p99Nanos,
                 long maxNanos, long totalNanos) {
            this.name = name;
            this.count = count;
            this.meanNanos = meanNanos;
            this.p50Nanos = p50Nanos;
            this.p95Nanos = p95Nanos;
            this.p99Nanos = p99Nanos;
            this.maxNanos = maxNanos;
            this.totalNanos = totalNanos;
        }

        public String name() {
            return name;
        }

        public long count() {
            return count;
        }

        public double meanMillis() {
            return meanNanos / 1.0e6;
        }

        public double p50Millis() {
            return p50Nanos / 1.0e6;
        }

        public double p95Millis() {
            return p95Nanos / 1.0e6;
        }

        public double p99Millis() {
            return p99Nanos / 1.0e6;
        }

        public double maxMillis() {
            return maxNanos / 1.0e6;
        }

        public double totalMillis() {
            return totalNanos / 1.0e6;
        }

        @Override
        public String toString() {
            return String.format("%s: n=%d mean=%.2fms p50=%.2fms p95=%.2fms p99=%.2fms max=%.2fms",
                    name, count, meanMillis(), p50Millis(), p95Millis(), p99Millis(), maxMillis());
        }
    }
}
