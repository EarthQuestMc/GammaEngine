package io.github.gammaengine.concurrent;

import io.github.gammaengine.GammaEngine;

import java.util.concurrent.ThreadFactory;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Thread factory producing predictably named engine threads.
 *
 * <p>Names matter more than usual here: the watchdog, the deadlock detector and every crash report
 * identify the guilty worker by thread name, so a thread called {@code GammaEngine-RegionTick-4}
 * is the difference between a usable report and a guess. Threads are daemons so a failed boot can
 * never leave a server process alive, and their priority is one notch below the main server thread
 * so that the coordinator always wins a contended scheduling decision.
 */
public final class GammaThreadFactory implements ThreadFactory {
    private final String prefix;
    private final int priority;
    private final AtomicInteger counter = new AtomicInteger();

    public GammaThreadFactory(String prefix) {
        this(prefix, Thread.NORM_PRIORITY - 1);
    }

    public GammaThreadFactory(String prefix, int priority) {
        this.prefix = prefix;
        this.priority = Math.max(Thread.MIN_PRIORITY, Math.min(Thread.MAX_PRIORITY, priority));
    }

    @Override
    public Thread newThread(Runnable runnable) {
        Thread thread = new Thread(runnable, GammaEngine.NAME + "-" + prefix + "-" + counter.incrementAndGet());
        thread.setDaemon(true);
        thread.setPriority(priority);
        thread.setUncaughtExceptionHandler(new Thread.UncaughtExceptionHandler() {
            @Override
            public void uncaughtException(Thread failed, Throwable error) {
                // Never let a worker die silently: a pool that quietly loses threads degrades into
                // a single-threaded server with no visible cause.
                GammaEngine.LOGGER.error("Uncaught exception on " + failed.getName(), error);
            }
        });
        return thread;
    }
}
