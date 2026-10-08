package io.github.gammaengine.diag;

import net.minecraft.server.MinecraftServer;
import net.minecraft.world.WorldServer;

import java.lang.management.BufferPoolMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryUsage;
import java.util.List;
import java.util.Locale;

/**
 * Where the server's memory actually is.
 *
 * <p>"The server uses too much RAM" is not actionable until the number is split. A JVM started with
 * {@code -Xms3G} reports three gigabytes of heap whether it needs them or not, and the memory an
 * operator sees in their hosting panel includes metaspace, thread stacks, Netty's off-heap arenas
 * and the mapped region files, none of which appear in the heap figure.
 *
 * <p>This report separates them, and can force a collection first so the heap figure means "what is
 * actually retained" rather than "what has not been collected yet".
 */
public final class MemoryReport {
    private MemoryReport() {
    }

    /**
     * Builds the report.
     *
     * @param collectFirst ask the JVM to collect before measuring. Only ever triggered by an
     *                     operator command: a periodic forced collection would cost more than it
     *                     reports.
     */
    public static String text(boolean collectFirst) {
        if (collectFirst) {
            System.gc();
            try {
                // Give the collector a moment; the numbers are meant to be read, not raced.
                Thread.sleep(250L);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            System.gc();
        }

        StringBuilder out = new StringBuilder(1024);
        MemoryUsage heap = ManagementFactory.getMemoryMXBean().getHeapMemoryUsage();
        MemoryUsage nonHeap = ManagementFactory.getMemoryMXBean().getNonHeapMemoryUsage();

        out.append("Memory").append(collectFirst ? " (after a forced collection)" : "").append('\n');
        out.append(String.format(Locale.ROOT, "  Heap:      %s used, %s committed, %s max%n",
                mib(heap.getUsed()), mib(heap.getCommitted()), mib(heap.getMax())));
        out.append(String.format(Locale.ROOT, "  Non-heap:  %s used, %s committed  (classes and metaspace)%n",
                mib(nonHeap.getUsed()), mib(nonHeap.getCommitted())));

        for (BufferPoolMXBean pool : ManagementFactory.getPlatformMXBeans(BufferPoolMXBean.class)) {
            out.append(String.format(Locale.ROOT, "  %-9s %s used across %d buffer(s)%n",
                    pool.getName() + ":", mib(pool.getMemoryUsed()), pool.getCount()));
        }

        Runtime runtime = Runtime.getRuntime();
        out.append(String.format(Locale.ROOT, "  Threads:   %d live%n", ManagementFactory.getThreadMXBean().getThreadCount()));
        out.append(String.format(Locale.ROOT, "  JVM:       %s free of %s allocated%n",
                mib(runtime.freeMemory()), mib(runtime.totalMemory())));

        MinecraftServer server = MinecraftServer.getServer();
        if (server != null && server.worldServers != null) {
            out.append("  Worlds:\n");
            long chunks = 0;
            long entities = 0;
            long tiles = 0;
            for (WorldServer world : server.worldServers) {
                if (world == null) {
                    continue;
                }
                int loaded = world.theChunkProviderServer == null ? 0
                        : world.theChunkProviderServer.getLoadedChunkCount();
                chunks += loaded;
                entities += world.loadedEntityList.size();
                tiles += world.loadedTileEntityList.size();
                out.append(String.format(Locale.ROOT, "    %-16s %5d chunks, %5d entities, %5d tile entities%n",
                        world.getWorldInfo().getWorldName() + "/" + world.provider.getDimensionName(),
                        loaded, world.loadedEntityList.size(), world.loadedTileEntityList.size()));
            }
            // A loaded 1.7.10 chunk column costs roughly 50 to 100 kB of block data alone, before
            // entities and tile entities, which is why chunk count is the number that matters.
            out.append(String.format(Locale.ROOT, "    total            %5d chunks, %5d entities, %5d tile entities (~%s of block data)%n",
                    chunks, entities, tiles, mib(chunks * 70L * 1024L)));
        }
        return out.toString();
    }

    private static String mib(long bytes) {
        if (bytes < 0) {
            return "unbounded";
        }
        return String.format(Locale.ROOT, "%.1f MiB", bytes / 1048576.0);
    }

    /** Heap bytes in use, for metrics. */
    public static long heapUsed() {
        return ManagementFactory.getMemoryMXBean().getHeapMemoryUsage().getUsed();
    }

    /** Sum of the direct and mapped buffer pools, which is memory the heap figure never shows. */
    public static long offHeapUsed() {
        long total = 0;
        for (BufferPoolMXBean pool : ManagementFactory.getPlatformMXBeans(BufferPoolMXBean.class)) {
            total += Math.max(0L, pool.getMemoryUsed());
        }
        return total;
    }
}
