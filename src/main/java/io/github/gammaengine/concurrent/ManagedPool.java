package io.github.gammaengine.concurrent;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.profiler.Counter;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.LatencyHistogram;

import java.util.concurrent.Callable;
import java.util.concurrent.Future;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/**
 * An instrumented, resizable thread pool.
 *
 * <p>Every pool in the engine is one of these, because auto-scaling needs numbers that a plain
 * {@link java.util.concurrent.ExecutorService} does not expose: how long a task waited in the
 * queue before starting (the signal that says "add a worker"), how long it ran (the signal that
 * says "this work does not belong here"), and how deep the queue is right now.
 *
 * <p>Queue-wait time is measured, not guessed: the submission timestamp is captured when the task
 * is handed over and read again when it starts running.
 */
public final class ManagedPool {
    private final String name;
    private final ThreadPoolExecutor executor;
    private final LatencyHistogram queueWait;
    private final LatencyHistogram execution;
    private final Counter submitted;
    private final Counter rejected;
    private final int maxThreads;

    ManagedPool(String name, int threads, int maxThreads, int priority) {
        this.name = name;
        this.maxThreads = Math.max(threads, maxThreads);
        GammaProfiler profiler = GammaProfiler.get();
        this.queueWait = profiler.registry().histogram("pool." + name + ".queueWait");
        this.execution = profiler.registry().histogram("pool." + name + ".execution");
        this.submitted = profiler.registry().counter("pool." + name + ".submitted");
        this.rejected = profiler.registry().counter("pool." + name + ".rejected");
        // A fixed-size pool with an unbounded queue: the engine controls the worker count itself
        // through resize(), and a bounded queue would turn a load spike into dropped world work.
        this.executor = new ThreadPoolExecutor(threads, threads, 60L, TimeUnit.SECONDS,
                new LinkedBlockingQueue<Runnable>(), new GammaThreadFactory(name, priority));
        this.executor.allowCoreThreadTimeOut(false);
    }

    public String name() {
        return name;
    }

    /** Submits a task, or runs it on the calling thread if the pool is shutting down. */
    public void execute(final Runnable task) {
        final long submittedAt = System.nanoTime();
        submitted.increment();
        try {
            executor.execute(new Runnable() {
                @Override
                public void run() {
                    long startedAt = System.nanoTime();
                    queueWait.record(startedAt - submittedAt);
                    try {
                        task.run();
                    } catch (Throwable error) {
                        GammaEngine.LOGGER.error("Task failed on pool " + name, error);
                    } finally {
                        execution.record(System.nanoTime() - startedAt);
                    }
                }
            });
        } catch (RejectedExecutionException e) {
            // Shutting down, or a resize race. Running inline is always correct here: the caller
            // already decided this work must happen, and losing it would desynchronize the world.
            rejected.increment();
            task.run();
        }
    }

    public <T> Future<T> submit(Callable<T> task) {
        submitted.increment();
        return executor.submit(task);
    }

    /** Current number of worker threads. */
    public int threads() {
        return executor.getCorePoolSize();
    }

    /** Upper bound the auto-scaler may not exceed. */
    public int maxThreads() {
        return maxThreads;
    }

    /** Number of workers currently running a task. */
    public int activeThreads() {
        return executor.getActiveCount();
    }

    /** Number of tasks waiting to start. */
    public int queueDepth() {
        return executor.getQueue().size();
    }

    public long completedTasks() {
        return executor.getCompletedTaskCount();
    }

    public LatencyHistogram queueWaitHistogram() {
        return queueWait;
    }

    public LatencyHistogram executionHistogram() {
        return execution;
    }

    /**
     * Changes the worker count. Growing takes effect on the next submission, shrinking lets the
     * extra workers finish what they are running first, so a resize never interrupts world work.
     */
    public void resize(int threads) {
        int target = Math.max(1, Math.min(maxThreads, threads));
        if (target == executor.getCorePoolSize()) {
            return;
        }
        if (target > executor.getCorePoolSize()) {
            executor.setMaximumPoolSize(target);
            executor.setCorePoolSize(target);
        } else {
            executor.setCorePoolSize(target);
            executor.setMaximumPoolSize(target);
        }
    }

    /** Waits for queued work to finish, then stops the workers. */
    public void shutdown(long timeoutMillis) {
        executor.shutdown();
        try {
            if (!executor.awaitTermination(timeoutMillis, TimeUnit.MILLISECONDS)) {
                GammaEngine.LOGGER.warn("Pool {} still had {} task(s) queued at shutdown", name, queueDepth());
                executor.shutdownNow();
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            executor.shutdownNow();
        }
    }

    @Override
    public String toString() {
        return String.format("%s: %d/%d threads, %d active, %d queued, %d completed",
                name, threads(), maxThreads, activeThreads(), queueDepth(), completedTasks());
    }
}
