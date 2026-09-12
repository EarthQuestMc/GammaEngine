package io.github.gammaengine.profiler;

import java.util.concurrent.atomic.AtomicLong;

/**
 * A named, thread-safe event counter.
 *
 * <p>Used for things the histogram cannot express: conflicts detected, tasks serialized,
 * cross-region transactions committed, chunks saved, JNI calls issued. Counters are monotonic
 * between two resets so a report can always show both the lifetime value and the value collected
 * during a profiling session.
 */
public final class Counter {
    private final String name;
    private final AtomicLong value = new AtomicLong();

    public Counter(String name) {
        this.name = name;
    }

    public String name() {
        return name;
    }

    public void increment() {
        value.incrementAndGet();
    }

    public void add(long delta) {
        value.addAndGet(delta);
    }

    public long get() {
        return value.get();
    }

    public void reset() {
        value.set(0);
    }

    @Override
    public String toString() {
        return name + "=" + value.get();
    }
}
