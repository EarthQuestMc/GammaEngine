package io.github.gammaengine.profiler;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

public class LatencyHistogramTest {

    @Test
    public void bucketIndicesAreStrictlyOrdered() {
        // Any two values whose order matters must never land in a bucket that inverts them,
        // otherwise reported percentiles would be meaningless.
        long previousValue = 0;
        int previousIndex = LatencyHistogram.indexOf(0);
        for (long value = 1; value < 5_000_000_000L; value = value + 1 + value / 97) {
            int index = LatencyHistogram.indexOf(value);
            assertTrue("index went backwards between " + previousValue + " and " + value,
                    index >= previousIndex);
            previousIndex = index;
            previousValue = value;
        }
    }

    @Test
    public void bucketBoundsContainTheirValues() {
        for (long value = 1; value < 1_000_000_000L; value = value + 1 + value / 31) {
            int index = LatencyHistogram.indexOf(value);
            long lower = LatencyHistogram.lowerBoundOf(index);
            assertTrue(value + " below its own bucket lower bound " + lower, lower <= value);
        }
    }

    @Test
    public void countMeanAndMaxAreExact() {
        LatencyHistogram histogram = new LatencyHistogram("test");
        histogram.record(1_000_000L);
        histogram.record(3_000_000L);
        histogram.record(2_000_000L);

        assertEquals(3, histogram.count());
        assertEquals(3_000_000L, histogram.maxNanos());
        assertEquals(2_000_000.0, histogram.meanNanos(), 0.001);
        assertEquals(6_000_000L, histogram.totalNanos());
    }

    @Test
    public void percentilesStayWithinTheDocumentedError() {
        LatencyHistogram histogram = new LatencyHistogram("test");
        // 1..1000 ms, uniform: p50 is 500ms, p95 is 950ms, p99 is 990ms.
        for (int millis = 1; millis <= 1000; millis++) {
            histogram.record(millis * 1_000_000L);
        }
        assertWithin(500.0, histogram.percentileNanos(50) / 1.0e6, 0.04);
        assertWithin(950.0, histogram.percentileNanos(95) / 1.0e6, 0.04);
        assertWithin(990.0, histogram.percentileNanos(99) / 1.0e6, 0.04);
    }

    @Test
    public void percentilesHandleSingleValueAndEmptyHistograms() {
        LatencyHistogram empty = new LatencyHistogram("empty");
        assertEquals(0L, empty.percentileNanos(99));
        assertEquals(0L, empty.count());

        LatencyHistogram single = new LatencyHistogram("single");
        single.record(42L);
        assertEquals(42L, single.percentileNanos(50));
        assertEquals(42L, single.percentileNanos(99));
    }

    @Test
    public void negativeDurationsAreClampedInsteadOfCorruptingTheHistogram() {
        LatencyHistogram histogram = new LatencyHistogram("test");
        histogram.record(-5L);
        assertEquals(1, histogram.count());
        assertEquals(0L, histogram.maxNanos());
    }

    @Test
    public void resetClearsEverything() {
        LatencyHistogram histogram = new LatencyHistogram("test");
        for (int i = 0; i < 100; i++) {
            histogram.record(i * 1000L);
        }
        histogram.reset();
        assertEquals(0, histogram.count());
        assertEquals(0L, histogram.maxNanos());
        assertEquals(0L, histogram.percentileNanos(99));
    }

    @Test
    public void concurrentRecordingLosesNoSample() throws Exception {
        final LatencyHistogram histogram = new LatencyHistogram("test");
        final int threads = 4;
        final int perThread = 20_000;
        Thread[] workers = new Thread[threads];
        for (int t = 0; t < threads; t++) {
            workers[t] = new Thread(new Runnable() {
                @Override
                public void run() {
                    for (int i = 0; i < perThread; i++) {
                        histogram.record(i);
                    }
                }
            });
        }
        for (Thread worker : workers) {
            worker.start();
        }
        for (Thread worker : workers) {
            worker.join();
        }
        assertEquals((long) threads * perThread, histogram.count());
    }

    private static void assertWithin(double expected, double actual, double relativeError) {
        double allowed = Math.abs(expected) * relativeError;
        assertTrue("expected " + expected + " +/- " + allowed + " but was " + actual,
                Math.abs(expected - actual) <= allowed);
    }
}
