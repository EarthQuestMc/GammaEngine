package io.github.gammaengine.profiler;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Registry of every histogram and counter published by the engine.
 *
 * <p>Subsystems ask for their metric once, keep the reference and record into it; lookups by name
 * are not on any hot path. The registry is what the {@code /autothread} command and the JSON
 * report read from, and it is deliberately the only place that knows the full metric list, so a
 * new subsystem becomes visible in every report by registering a single metric.
 */
public final class MetricsRegistry {
    private final ConcurrentHashMap<String, LatencyHistogram> histograms =
            new ConcurrentHashMap<String, LatencyHistogram>();
    private final ConcurrentHashMap<String, Counter> counters = new ConcurrentHashMap<String, Counter>();

    /** Returns the histogram with this name, creating it on first use. */
    public LatencyHistogram histogram(String name) {
        LatencyHistogram existing = histograms.get(name);
        if (existing != null) {
            return existing;
        }
        LatencyHistogram created = new LatencyHistogram(name);
        LatencyHistogram raced = histograms.putIfAbsent(name, created);
        return raced == null ? created : raced;
    }

    /** Returns the counter with this name, creating it on first use. */
    public Counter counter(String name) {
        Counter existing = counters.get(name);
        if (existing != null) {
            return existing;
        }
        Counter created = new Counter(name);
        Counter raced = counters.putIfAbsent(name, created);
        return raced == null ? created : raced;
    }

    public Collection<LatencyHistogram> histograms() {
        return Collections.unmodifiableCollection(histograms.values());
    }

    public Collection<Counter> counters() {
        return Collections.unmodifiableCollection(counters.values());
    }

    /** Histogram snapshots ordered by total time spent, which is the order a reader cares about. */
    public List<LatencyHistogram.Snapshot> snapshotsByCost() {
        List<LatencyHistogram.Snapshot> out = new ArrayList<LatencyHistogram.Snapshot>(histograms.size());
        for (LatencyHistogram histogram : histograms.values()) {
            if (histogram.count() > 0) {
                out.add(histogram.snapshot());
            }
        }
        Collections.sort(out, new Comparator<LatencyHistogram.Snapshot>() {
            @Override
            public int compare(LatencyHistogram.Snapshot a, LatencyHistogram.Snapshot b) {
                return Double.compare(b.totalMillis(), a.totalMillis());
            }
        });
        return out;
    }

    /** Counter values ordered by name. */
    public Map<String, Long> counterValues() {
        Map<String, Long> out = new java.util.TreeMap<String, Long>();
        for (Map.Entry<String, Counter> entry : counters.entrySet()) {
            out.put(entry.getKey(), entry.getValue().get());
        }
        return out;
    }

    /** Clears every metric. Called when a profiling session starts. */
    public void reset() {
        for (LatencyHistogram histogram : histograms.values()) {
            histogram.reset();
        }
        for (Counter counter : counters.values()) {
            counter.reset();
        }
    }
}
