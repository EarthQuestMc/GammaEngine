package io.github.gammaengine.profiler;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

public class TickStatisticsTest {

    @Test
    public void reportsNothingBeforeTheFirstTick() {
        assertNull(new TickStatistics().mspt());
    }

    @Test
    public void percentilesOverTheRollingWindowAreExact() {
        TickStatistics stats = new TickStatistics();
        for (int millis = 1; millis <= 100; millis++) {
            stats.recordTick(millis * 1_000_000L);
        }
        TickStatistics.Mspt mspt = stats.mspt();
        assertNotNull(mspt);
        assertEquals(100, mspt.samples());
        assertEquals(50.0, mspt.p50(), 0.001);
        assertEquals(95.0, mspt.p95(), 0.001);
        assertEquals(99.0, mspt.p99(), 0.001);
        assertEquals(100.0, mspt.max(), 0.001);
        assertEquals(50.5, mspt.mean(), 0.001);
    }

    @Test
    public void theWindowForgetsOldTicks() {
        TickStatistics stats = new TickStatistics();
        // Fill the window with slow ticks, then overwrite it entirely with fast ones: a recovered
        // server must stop reporting the spike it recovered from.
        for (int i = 0; i < 1200; i++) {
            stats.recordTick(200_000_000L);
        }
        for (int i = 0; i < 1200; i++) {
            stats.recordTick(5_000_000L);
        }
        TickStatistics.Mspt mspt = stats.mspt();
        assertEquals(5.0, mspt.max(), 0.001);
        assertEquals(2400L, stats.totalTicks());
    }

    @Test
    public void tpsNeverExceedsTwenty() {
        TickStatistics stats = new TickStatistics();
        for (int i = 0; i < 50; i++) {
            stats.recordTick(1_000L);
        }
        assertTrue("tps was " + stats.tps1m(), stats.tps1m() <= 20.0001);
        assertTrue("tps was " + stats.tps5m(), stats.tps5m() <= 20.0001);
        assertTrue("tps was " + stats.tps15m(), stats.tps15m() <= 20.0001);
    }
}
