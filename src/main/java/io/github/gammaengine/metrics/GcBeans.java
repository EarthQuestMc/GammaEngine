package io.github.gammaengine.metrics;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;

/**
 * Tells stop-the-world pauses from concurrent cycles among the JVM's garbage collector beans.
 *
 * <p>Concurrent collectors publish one bean for their pauses and one for their cycles: ZGC has
 * {@code ZGC Pauses} and {@code ZGC Cycles} ({@code ZGC Minor Pauses}, {@code ZGC Major Cycles}... when
 * generational), Shenandoah {@code Shenandoah Pauses} and {@code Shenandoah Cycles}, and G1 adds
 * {@code G1 Concurrent GC} from Java 20. A cycle runs beside the server and lasts hundreds of
 * milliseconds: counting it as a pause reports stalls the server never had. The beans of Parallel,
 * Serial and G1's young and old collections are pauses. CMS's {@code ConcurrentMarkSweep} mixes both
 * and is counted as pauses, which overstates them.
 *
 * <p>JMX reports whole milliseconds: a ZGC pause of 0.2 ms reads 0. Checking "pauses under 10 ms" to
 * a tenth of a millisecond needs the JVM's own log, {@code -Xlog:safepoint}.
 */
public final class GcBeans {
    private GcBeans() {
    }

    /** True for a bean whose collections run concurrently with the application. */
    public static boolean isConcurrent(String collectorName) {
        return collectorName.endsWith(" Cycles") || "G1 Concurrent GC".equals(collectorName);
    }

    /** Cumulative pauses and cycles since the JVM started, summed over every collector bean. */
    public static Totals totals() {
        long pauses = 0;
        long pauseMillis = 0;
        long cycles = 0;
        long cycleMillis = 0;
        for (GarbageCollectorMXBean bean : ManagementFactory.getGarbageCollectorMXBeans()) {
            // -1 means the bean does not know; it must not subtract from the others.
            long count = Math.max(0L, bean.getCollectionCount());
            long millis = Math.max(0L, bean.getCollectionTime());
            if (isConcurrent(bean.getName())) {
                cycles += count;
                cycleMillis += millis;
            } else {
                pauses += count;
                pauseMillis += millis;
            }
        }
        return new Totals(pauses, pauseMillis, cycles, cycleMillis);
    }

    /** Collection counts and times, either cumulative or the difference between two readings. */
    public static final class Totals {
        public final long pauses;
        public final long pauseMillis;
        public final long cycles;
        public final long cycleMillis;

        public Totals(long pauses, long pauseMillis, long cycles, long cycleMillis) {
            this.pauses = pauses;
            this.pauseMillis = pauseMillis;
            this.cycles = cycles;
            this.cycleMillis = cycleMillis;
        }

        /** What happened between {@code earlier} and this reading. */
        public Totals since(Totals earlier) {
            return new Totals(pauses - earlier.pauses, pauseMillis - earlier.pauseMillis,
                    cycles - earlier.cycles, cycleMillis - earlier.cycleMillis);
        }

        /** "3 pause(s), 7 ms", followed by the concurrent cycles when the collector has any. */
        public String describe() {
            String text = pauses + " pause(s), " + pauseMillis + " ms";
            return cycles > 0 ? text + "; " + cycles + " concurrent cycle(s), " + cycleMillis + " ms" : text;
        }
    }
}
