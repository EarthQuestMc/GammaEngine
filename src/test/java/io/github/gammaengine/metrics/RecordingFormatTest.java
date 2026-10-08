package io.github.gammaengine.metrics;

import io.github.gammaengine.profiler.TickStatistics;
import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;

/** The parts of a bench recording that do not need a running server. */
public class RecordingFormatTest {

    @Test
    public void jsonStringsAreEscaped() {
        assertEquals("\"a\\\"b\\\\c\"", Recording.quote("a\"b\\c"));
        assertEquals("\"line\\u000abreak\"", Recording.quote("line\nbreak"));
        assertEquals("null", Recording.quote(null));
    }

    @Test
    public void csvFieldsAreQuoted() {
        assertEquals("\"end of minor GC\"", Recording.csv("end of minor GC"));
        assertEquals("\"say \"\"hi\"\"\"", Recording.csv("say \"hi\""));
        assertEquals("", Recording.csv(null));
    }

    @Test
    public void ticksHeaderHasOneColumnPerRecordedValuePlusOtherTime() {
        // Each tick row holds 11 values; the writer inserts other_ns after worlds_ns.
        assertEquals(12, Recording.TICKS_HEADER.split(",").length);
    }

    @Test
    public void msptSummaryIsExactOverTheGivenTicks() {
        long[] nanos = new long[200];
        for (int i = 0; i < 100; i++) {
            nanos[i] = (i + 1) * 1_000_000L;
        }
        TickStatistics.Mspt mspt = TickStatistics.Mspt.of(nanos, 100);
        assertEquals(100, mspt.samples());
        assertEquals(50.5, mspt.mean(), 1e-9);
        assertEquals(50.0, mspt.p50(), 1e-9);
        assertEquals(95.0, mspt.p95(), 1e-9);
        assertEquals(99.0, mspt.p99(), 1e-9);
        assertEquals(100.0, mspt.max(), 1e-9);
        assertNull(TickStatistics.Mspt.of(nanos, 0));
    }
}
