package io.github.gammaengine.profiler;

import org.junit.Test;

import static org.junit.Assert.assertEquals;

/** The short windows behind /tps: counted TPS and MSPT of the most recent ticks. */
public class TickWindowsTest {
    private static final long MS = 1_000_000L;

    @Test
    public void steadyTwentyTicksPerSecond() {
        TickStatistics ticks = new TickStatistics();
        long now = 0;
        for (int i = 0; i < 400; i++) {
            now += 50 * MS;
            ticks.recordTick(2 * MS, now);
        }
        assertEquals(20.0, ticks.tpsOver(5, now), 0.01);
        assertEquals(20.0, ticks.tpsOver(10, now), 0.01);
    }

    @Test
    public void aStallShowsInTheShortWindowFirst() {
        TickStatistics ticks = new TickStatistics();
        long now = 0;
        for (int i = 0; i < 400; i++) {
            now += 50 * MS;
            ticks.recordTick(2 * MS, now);
        }
        // Two seconds without a single tick.
        now += 2000 * MS;
        assertEquals(12.0, ticks.tpsOver(5, now), 0.01);
        assertEquals(16.0, ticks.tpsOver(10, now), 0.01);
    }

    @Test
    public void youngServerIsMeasuredOverWhatItHas() {
        TickStatistics ticks = new TickStatistics();
        long now = 0;
        for (int i = 0; i < 41; i++) {
            now += 50 * MS;
            ticks.recordTick(MS, now);
        }
        assertEquals(20.0, ticks.tpsOver(60, now), 0.01);
    }

    @Test
    public void msptOfLastLooksOnlyAtTheNewestTicks() {
        TickStatistics ticks = new TickStatistics();
        long now = 0;
        for (int i = 0; i < 300; i++) {
            now += 50 * MS;
            ticks.recordTick(i < 100 ? 40 * MS : 2 * MS, now);
        }
        TickStatistics.Mspt last = ticks.msptOfLast(200);
        assertEquals(200, last.samples());
        assertEquals(2.0, last.max(), 1e-9);
        assertEquals(2.0, last.min(), 1e-9);
        assertEquals(40.0, ticks.msptOfLast(300).max(), 1e-9);
    }
}
