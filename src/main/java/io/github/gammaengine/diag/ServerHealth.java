package io.github.gammaengine.diag;

import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.TickStatistics;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.WorldServer;
import net.minecraftforge.common.DimensionManager;
import org.bukkit.ChatColor;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.OperatingSystemMXBean;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;

/**
 * The server health report behind {@code /tps}: tick rate and tick time over several windows,
 * CPU, memory, garbage collection, and what is loaded, in the spirit of Spark's {@code /tps}.
 *
 * <p>CPU and GC are sampled once a second by a daemon thread: reading the system CPU load can take
 * milliseconds on Windows and must never run on the server thread. Thread invariant: the sampler
 * writes the rings under the instance lock; readers take the same lock. Tick and world figures are
 * read by the report itself, on the thread running the command, which is the server thread.
 */
public final class ServerHealth {
    private static final ServerHealth INSTANCE = new ServerHealth();
    /** Fifteen minutes of one-second samples. */
    private static final int SAMPLES = 900;
    private static final double BUDGET_MS = 50.0;

    private final double[] processCpu = new double[SAMPLES];
    private final double[] systemCpu = new double[SAMPLES];
    private final long[] gcCount = new long[SAMPLES];
    private final long[] gcMillis = new long[SAMPLES];
    private int cursor;
    private int samples;
    private Thread sampler;

    private ServerHealth() {
    }

    public static ServerHealth get() {
        return INSTANCE;
    }

    /** Starts the once-a-second sampler. Safe to call more than once. */
    public synchronized void start() {
        if (sampler != null) {
            return;
        }
        sampler = new Thread(new Runnable() {
            @Override
            public void run() {
                while (!Thread.currentThread().isInterrupted()) {
                    sample();
                    try {
                        Thread.sleep(1000L);
                    } catch (InterruptedException e) {
                        return;
                    }
                }
            }
        }, "GammaEngine-HealthSampler");
        sampler.setDaemon(true);
        sampler.setPriority(Thread.MIN_PRIORITY + 1);
        sampler.start();
    }

    public synchronized void stop() {
        if (sampler != null) {
            sampler.interrupt();
            sampler = null;
        }
    }

    private void sample() {
        double process = -1.0;
        double system = -1.0;
        OperatingSystemMXBean os = ManagementFactory.getOperatingSystemMXBean();
        if (os instanceof com.sun.management.OperatingSystemMXBean) {
            com.sun.management.OperatingSystemMXBean hotspot = (com.sun.management.OperatingSystemMXBean) os;
            process = hotspot.getProcessCpuLoad();
            system = hotspot.getSystemCpuLoad();
        }
        long count = 0;
        long millis = 0;
        for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
            count += Math.max(0L, gc.getCollectionCount());
            millis += Math.max(0L, gc.getCollectionTime());
        }
        synchronized (this) {
            processCpu[cursor] = process;
            systemCpu[cursor] = system;
            gcCount[cursor] = count;
            gcMillis[cursor] = millis;
            cursor = (cursor + 1) % SAMPLES;
            if (samples < SAMPLES) {
                samples++;
            }
        }
    }

    /** Mean CPU load over the last {@code seconds} samples, 0 to 1, or a negative value when unknown. */
    private synchronized double cpu(double[] ring, int seconds) {
        int count = Math.min(seconds, samples);
        double sum = 0.0;
        int known = 0;
        for (int i = 0; i < count; i++) {
            double value = ring[Math.floorMod(cursor - 1 - i, SAMPLES)];
            if (value >= 0.0) {
                sum += value;
                known++;
            }
        }
        return known == 0 ? -1.0 : sum / known;
    }

    /** Collections and collection time over the last {@code seconds}, from cumulative counters. */
    private synchronized long[] gcDelta(int seconds) {
        if (samples < 2) {
            return new long[]{0L, 0L};
        }
        int back = Math.min(seconds, samples - 1);
        int newest = Math.floorMod(cursor - 1, SAMPLES);
        int oldest = Math.floorMod(cursor - 1 - back, SAMPLES);
        return new long[]{gcCount[newest] - gcCount[oldest], gcMillis[newest] - gcMillis[oldest]};
    }

    /** The report lines, coloured for chat and console. Server thread. */
    public List<String> report() {
        List<String> lines = new ArrayList<String>();
        TickStatistics ticks = GammaProfiler.get().ticks();

        lines.add(ChatColor.GOLD + "TPS from last 5s, 10s, 1m, 5m, 15m: "
                + tps(ticks.tpsOver(5)) + sep() + tps(ticks.tpsOver(10)) + sep() + tps(ticks.tpsOver(60))
                + sep() + tps(ticks.tps5m()) + sep() + tps(ticks.tps15m()));

        TickStatistics.Mspt last10s = ticks.msptOfLast(200);
        TickStatistics.Mspt lastMinute = ticks.msptOfLast(1200);
        if (last10s != null && lastMinute != null) {
            lines.add(ChatColor.GOLD + "Tick durations (min/med/95%/max ms) from last 10s, 1m:");
            lines.add("  " + durations(last10s) + ChatColor.GRAY + "; " + durations(lastMinute));
            lines.add(ChatColor.GOLD + "Tick budget used, last 1m: " + ms(lastMinute.mean())
                    + ChatColor.GRAY + String.format(Locale.ROOT, " mean of %.0f ms (%.0f%%)", BUDGET_MS,
                    100.0 * lastMinute.mean() / BUDGET_MS));
        }

        double process10 = cpu(processCpu, 10);
        if (process10 >= 0.0) {
            lines.add(ChatColor.GOLD + "CPU usage from last 10s, 1m, 15m:");
            lines.add("  " + percent(process10) + sep() + percent(cpu(processCpu, 60)) + sep()
                    + percent(cpu(processCpu, 900)) + ChatColor.GRAY + " (process)");
            lines.add("  " + percent(cpu(systemCpu, 10)) + sep() + percent(cpu(systemCpu, 60)) + sep()
                    + percent(cpu(systemCpu, 900)) + ChatColor.GRAY + " (system)");
        }

        Runtime runtime = Runtime.getRuntime();
        long used = runtime.totalMemory() - runtime.freeMemory();
        long[] gc = gcDelta(60);
        lines.add(ChatColor.GOLD + "Memory: " + ChatColor.WHITE + mib(used) + ChatColor.GRAY + " used of "
                + mib(runtime.totalMemory()) + " allocated, " + mib(runtime.maxMemory()) + " max"
                + ChatColor.GOLD + "  GC last 1m: " + ChatColor.WHITE + gc[0] + ChatColor.GRAY + " collection(s), "
                + gc[1] + " ms");

        addLoad(lines, lastMinute);
        return lines;
    }

    private void addLoad(List<String> lines, TickStatistics.Mspt lastMinute) {
        MinecraftServer server = MinecraftServer.getServer();
        if (server == null) {
            return;
        }
        int players = server.getCurrentPlayerCount();
        long chunks = 0;
        long entities = 0;
        long tiles = 0;
        List<String[]> worlds = new ArrayList<String[]>();
        final List<Double> costs = new ArrayList<Double>();
        for (WorldServer world : DimensionManager.getWorlds()) {
            int worldChunks = world.theChunkProviderServer == null ? 0 : world.theChunkProviderServer.getLoadedChunkCount();
            chunks += worldChunks;
            entities += world.loadedEntityList.size();
            tiles += world.loadedTileEntityList.size();
            double meanMs = meanMillis(server.worldTickTimes.get(world.provider.dimensionId));
            costs.add(meanMs);
            worlds.add(new String[]{world.provider.getDimensionName() + " (" + world.provider.dimensionId + ")",
                    String.format(Locale.ROOT, "%.2f ms, %d chunks, %d entities, %d tile entities", meanMs,
                            worldChunks, world.loadedEntityList.size(), world.loadedTileEntityList.size())});
        }

        String perPlayer = players > 0 && lastMinute != null
                ? ChatColor.GRAY + String.format(Locale.ROOT, " (%.3f ms per player, budget %.3f ms)",
                lastMinute.mean() / players, BUDGET_MS / players)
                : "";
        lines.add(ChatColor.GOLD + "Load: " + ChatColor.WHITE + players + ChatColor.GRAY + " player(s), "
                + ChatColor.WHITE + chunks + ChatColor.GRAY + " chunks, " + ChatColor.WHITE + entities
                + ChatColor.GRAY + " entities, " + ChatColor.WHITE + tiles + ChatColor.GRAY + " tile entities" + perPlayer);

        // The three most expensive worlds, by mean tick time over the last 100 ticks.
        List<Integer> order = new ArrayList<Integer>();
        for (int i = 0; i < worlds.size(); i++) {
            order.add(i);
        }
        Collections.sort(order, new Comparator<Integer>() {
            @Override
            public int compare(Integer a, Integer b) {
                return Double.compare(costs.get(b), costs.get(a));
            }
        });
        lines.add(ChatColor.GOLD + "Worlds, mean tick over the last 100 ticks:");
        for (int i = 0; i < Math.min(3, order.size()); i++) {
            String[] world = worlds.get(order.get(i));
            lines.add("  " + ChatColor.WHITE + world[0] + ChatColor.GRAY + ": " + world[1]);
        }
    }

    private static double meanMillis(long[] times) {
        if (times == null || times.length == 0) {
            return 0.0;
        }
        long sum = 0;
        for (long time : times) {
            sum += time;
        }
        return sum / (double) times.length / 1.0e6;
    }

    private static String sep() {
        return ChatColor.GRAY + ", ";
    }

    private static String tps(double tps) {
        ChatColor colour = tps >= 19.0 ? ChatColor.GREEN : tps >= 15.0 ? ChatColor.YELLOW : ChatColor.RED;
        return colour + (tps > 20.05 ? "*" : "") + String.format(Locale.ROOT, "%.1f", Math.min(20.0, tps));
    }

    private static String ms(double millis) {
        ChatColor colour = millis <= 30.0 ? ChatColor.GREEN : millis <= BUDGET_MS ? ChatColor.YELLOW : ChatColor.RED;
        return colour + String.format(Locale.ROOT, "%.1f", millis);
    }

    private static String durations(TickStatistics.Mspt mspt) {
        String slash = ChatColor.GRAY + "/";
        return ms(mspt.min()) + slash + ms(mspt.p50()) + slash + ms(mspt.p95()) + slash + ms(mspt.max());
    }

    private static String percent(double load) {
        if (load < 0.0) {
            return ChatColor.GRAY + "n/a";
        }
        ChatColor colour = load < 0.7 ? ChatColor.GREEN : load < 0.9 ? ChatColor.YELLOW : ChatColor.RED;
        return colour + String.format(Locale.ROOT, "%.0f%%", load * 100.0);
    }

    private static String mib(long bytes) {
        return bytes >= 1L << 30
                ? String.format(Locale.ROOT, "%.1f GiB", bytes / (double) (1L << 30))
                : String.format(Locale.ROOT, "%d MiB", bytes >> 20);
    }
}
