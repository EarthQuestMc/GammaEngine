package io.github.gammaengine.metrics;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class GcBeansTest {
    @Test
    public void concurrentCyclesAreNotPauses() {
        assertTrue(GcBeans.isConcurrent("ZGC Cycles"));
        assertTrue(GcBeans.isConcurrent("ZGC Minor Cycles"));
        assertTrue(GcBeans.isConcurrent("ZGC Major Cycles"));
        assertTrue(GcBeans.isConcurrent("Shenandoah Cycles"));
        assertTrue(GcBeans.isConcurrent("G1 Concurrent GC"));
    }

    @Test
    public void stopTheWorldCollectorsArePauses() {
        assertFalse(GcBeans.isConcurrent("ZGC Pauses"));
        assertFalse(GcBeans.isConcurrent("ZGC Minor Pauses"));
        assertFalse(GcBeans.isConcurrent("ZGC Major Pauses"));
        assertFalse(GcBeans.isConcurrent("Shenandoah Pauses"));
        assertFalse(GcBeans.isConcurrent("G1 Young Generation"));
        assertFalse(GcBeans.isConcurrent("G1 Old Generation"));
        assertFalse(GcBeans.isConcurrent("PS Scavenge"));
        assertFalse(GcBeans.isConcurrent("PS MarkSweep"));
        assertFalse(GcBeans.isConcurrent("Copy"));
        assertFalse(GcBeans.isConcurrent("MarkSweepCompact"));
    }

    @Test
    public void differenceAndDescription() {
        GcBeans.Totals start = new GcBeans.Totals(10, 40, 2, 300);
        GcBeans.Totals end = new GcBeans.Totals(13, 47, 2, 300);
        GcBeans.Totals delta = end.since(start);
        assertEquals(3, delta.pauses);
        assertEquals(7, delta.pauseMillis);
        assertEquals("3 pause(s), 7 ms", delta.describe());

        GcBeans.Totals withCycles = new GcBeans.Totals(18, 1, 4, 912);
        assertEquals("18 pause(s), 1 ms; 4 concurrent cycle(s), 912 ms", withCycles.describe());
    }

    @Test
    public void totalsOfThisJvmAreNotNegative() {
        GcBeans.Totals totals = GcBeans.totals();
        assertTrue(totals.pauses >= 0 && totals.pauseMillis >= 0);
        assertTrue(totals.cycles >= 0 && totals.cycleMillis >= 0);
    }
}
