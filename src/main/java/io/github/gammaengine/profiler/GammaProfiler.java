package io.github.gammaengine.profiler;

import io.github.gammaengine.GammaEngine;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.Charset;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.List;
import java.util.Map;

/**
 * The engine profiler: one registry of metrics, one rolling tick view, and profiling sessions.
 *
 * <p>A session is just a reset of every metric plus a start timestamp. That is enough to answer
 * the only question benchmarking actually needs: between these two points in time, where did the
 * CPU go, and what was the tick time distribution. Reports are written both to the console and to
 * a file under {@code gammaengine/reports} so a benchmark run leaves an artifact behind.
 */
public final class GammaProfiler {
    private static final GammaProfiler INSTANCE = new GammaProfiler();

    private final MetricsRegistry registry = new MetricsRegistry();
    private final TickStatistics tickStatistics = new TickStatistics();

    private volatile boolean sessionActive;
    private volatile long sessionStartMillis;
    private volatile long sessionStartTicks;
    private volatile String lastReport;

    private GammaProfiler() {
    }

    public static GammaProfiler get() {
        return INSTANCE;
    }

    public MetricsRegistry registry() {
        return registry;
    }

    public TickStatistics ticks() {
        return tickStatistics;
    }

    /** Shorthand for the common "time this block" pattern. */
    public void record(String metric, long durationNanos) {
        registry.histogram(metric).record(durationNanos);
    }

    public void count(String metric) {
        registry.counter(metric).increment();
    }

    public boolean sessionActive() {
        return sessionActive;
    }

    /**
     * Starts a profiling session, clearing every metric.
     *
     * @return false when a session was already running, in which case nothing is cleared
     */
    public boolean startSession() {
        if (sessionActive) {
            return false;
        }
        registry.reset();
        sessionStartMillis = System.currentTimeMillis();
        sessionStartTicks = tickStatistics.totalTicks();
        sessionActive = true;
        return true;
    }

    /**
     * Stops the running session and builds its report.
     *
     * @return the report text, or {@code null} when no session was running
     */
    public String stopSession() {
        if (!sessionActive) {
            return null;
        }
        sessionActive = false;
        String report = buildReport();
        lastReport = report;
        return report;
    }

    /** The report of the last finished session, or {@code null}. */
    public String lastReport() {
        return lastReport;
    }

    /** Builds a report of the current metric values, whether or not a session is running. */
    public String buildReport() {
        StringBuilder out = new StringBuilder(4096);
        long durationMillis = Math.max(1L, System.currentTimeMillis() - sessionStartMillis);
        long ticks = tickStatistics.totalTicks() - sessionStartTicks;

        out.append("=== ").append(GammaEngine.NAME).append(" profiling report ===\n");
        out.append(String.format("Duration: %.1f s over %d ticks%n", durationMillis / 1000.0, ticks));
        out.append(String.format("TPS: %.2f (1m) %.2f (5m) %.2f (15m)%n",
                tickStatistics.tps1m(), tickStatistics.tps5m(), tickStatistics.tps15m()));

        TickStatistics.Mspt mspt = tickStatistics.mspt();
        if (mspt != null) {
            out.append("MSPT (last minute): ").append(mspt).append('\n');
        }

        List<LatencyHistogram.Snapshot> snapshots = registry.snapshotsByCost();
        if (!snapshots.isEmpty()) {
            out.append("\nTime by subsystem (total cost first):\n");
            for (LatencyHistogram.Snapshot snapshot : snapshots) {
                out.append(String.format("  %-38s n=%-10d total=%10.1fms mean=%7.3fms p95=%7.3fms p99=%7.3fms max=%8.3fms%n",
                        snapshot.name(), snapshot.count(), snapshot.totalMillis(), snapshot.meanMillis(),
                        snapshot.p95Millis(), snapshot.p99Millis(), snapshot.maxMillis()));
            }
        }

        Map<String, Long> counters = registry.counterValues();
        if (!counters.isEmpty()) {
            out.append("\nCounters:\n");
            for (Map.Entry<String, Long> entry : counters.entrySet()) {
                out.append(String.format("  %-38s %d%n", entry.getKey(), entry.getValue()));
            }
        }
        return out.toString();
    }

    /**
     * Writes a report next to the world folder so benchmark runs leave a durable artifact.
     *
     * @return the file that was written, or {@code null} when writing failed
     */
    public File writeReport(String report) {
        File directory = new File("gammaengine/reports");
        if (!directory.isDirectory() && !directory.mkdirs()) {
            GammaEngine.LOGGER.warn("Could not create {}, profiling report not saved", directory);
            return null;
        }
        String stamp = new SimpleDateFormat("yyyyMMdd-HHmmss").format(new Date());
        File file = new File(directory, "profile-" + stamp + ".txt");
        Writer writer = null;
        try {
            writer = new OutputStreamWriter(new FileOutputStream(file), Charset.forName("UTF-8"));
            writer.write(report);
            return file;
        } catch (IOException e) {
            GammaEngine.LOGGER.warn("Could not write profiling report to " + file, e);
            return null;
        } finally {
            if (writer != null) {
                try {
                    writer.close();
                } catch (IOException ignored) {
                    // report already flushed or lost, nothing else to do
                }
            }
        }
    }
}
