package io.github.gammaengine.metrics;

import com.sun.management.GarbageCollectionNotificationInfo;
import com.sun.management.GcInfo;
import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.profiler.TickStatistics;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.WorldServer;
import net.minecraftforge.common.DimensionManager;

import javax.management.ListenerNotFoundException;
import javax.management.Notification;
import javax.management.NotificationEmitter;
import javax.management.NotificationListener;
import javax.management.openmbean.CompositeData;
import java.io.BufferedWriter;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryUsage;
import java.nio.charset.StandardCharsets;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicLong;

/**
 * One recording session: every tick and every garbage collection between start and stop, written
 * to {@code gammaengine/bench/<name>-<date>/}. With attribution, the session also owns the
 * {@link AttributionTable} that {@link TickAttribution} fills, and writes {@code mods.csv} and
 * {@code chunks.csv} when it ends.
 *
 * <p>Thread invariants:
 * <ul>
 *   <li>{@link #onTick} runs on the server thread only; it reads the world lists, which only that
 *       thread writes, and fills {@code durations}, which only it reads until {@link #finish}.</li>
 *   <li>GC notifications arrive on a JMX thread and only touch the queue and the atomic counters.</li>
 *   <li>All file writing happens on the writer thread, so the tick never waits on the disk.</li>
 *   <li>{@link #finish} runs after the recorder stopped handing ticks to this session.</li>
 *   <li>{@link #attribution} is written by the server thread through {@link TickAttribution} and
 *       read by {@link #finish} only after the recorder detached it and, from another thread,
 *       waited for the server thread to start a new tick.</li>
 * </ul>
 */
final class Recording {
    static final String TICKS_HEADER = "tick,time_ms,tick_ns,worlds_ns,other_ns,players,chunks,entities,"
            + "tile_entities,entities_ticked,tiles_ticked,heap_used_bytes";
    static final String GC_HEADER = "time_ms,collector,action,cause,duration_ms,heap_before_bytes,heap_after_bytes";

    private static final long WRITE_INTERVAL_MILLIS = 500L;

    final String name;
    final File directory;
    /** Level 2 sums, or {@code null} when this session does not attribute. */
    final AttributionTable attribution;
    private final long startMillis = System.currentTimeMillis();
    private final long startNanos = System.nanoTime();

    private boolean started;
    private long[] durations = new long[20 * 60 * 10];
    private int ticks;
    private long playerSum;
    private int playerMax;

    private final ConcurrentLinkedQueue<long[]> tickRows = new ConcurrentLinkedQueue<long[]>();
    private final ConcurrentLinkedQueue<String> gcRows = new ConcurrentLinkedQueue<String>();
    private final AtomicLong gcCount = new AtomicLong();
    private final AtomicLong gcTotalMillis = new AtomicLong();
    private final AtomicLong gcMaxMillis = new AtomicLong();
    private final List<Runnable> gcUnsubscribers = new ArrayList<Runnable>();

    private final Writer ticksOut;
    private final Writer gcOut;
    private final Thread writer;
    private volatile boolean stopping;
    private volatile boolean writeFailed;

    Recording(String name, boolean attribution) throws IOException {
        this.name = name;
        this.attribution = attribution ? new AttributionTable() : null;
        SimpleDateFormat stamp = new SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT);
        this.directory = new File(new File("gammaengine", "bench"), name + "-" + stamp.format(new Date(startMillis)));
        if (!directory.isDirectory() && !directory.mkdirs()) {
            throw new IOException("cannot create " + directory);
        }
        ticksOut = open("ticks.csv", TICKS_HEADER);
        gcOut = open("gc.csv", GC_HEADER);
        subscribeToGarbageCollections();

        writer = new Thread(new Runnable() {
            @Override
            public void run() {
                drainLoop();
            }
        }, "GammaEngine-BenchWriter");
        writer.setDaemon(true);
        writer.setPriority(Thread.MIN_PRIORITY + 1);
        writer.start();
    }

    private Writer open(String file, String header) throws IOException {
        Writer out = open(file);
        out.write(header);
        out.write('\n');
        return out;
    }

    private Writer open(String file) throws IOException {
        return new BufferedWriter(new OutputStreamWriter(
                new FileOutputStream(new File(directory, file)), StandardCharsets.UTF_8));
    }

    /** Server thread, end of every tick. */
    void onTick(long durationNanos) {
        if (!started) {
            // The tick that started the session also paid for creating it (files, writer thread).
            started = true;
            return;
        }
        MinecraftServer server = MinecraftServer.getServer();
        int tick = server.getTickCounter();
        long worldsNanos = 0;
        long chunks = 0;
        long entities = 0;
        long tiles = 0;
        long entitiesTicked = 0;
        long tilesTicked = 0;
        for (WorldServer world : DimensionManager.getWorlds()) {
            long[] times = server.worldTickTimes.get(world.provider.dimensionId);
            if (times != null) {
                worldsNanos += times[tick % times.length];
            }
            if (world.theChunkProviderServer != null) {
                chunks += world.theChunkProviderServer.getLoadedChunkCount();
            }
            entities += world.loadedEntityList.size();
            tiles += world.loadedTileEntityList.size();
            entitiesTicked += world.entitiesTicked;
            tilesTicked += world.tilesTicked;
        }
        int players = server.getCurrentPlayerCount();
        Runtime runtime = Runtime.getRuntime();

        tickRows.add(new long[]{tick, System.currentTimeMillis(), durationNanos, worldsNanos, players, chunks,
                entities, tiles, entitiesTicked, tilesTicked, runtime.totalMemory() - runtime.freeMemory()});

        if (ticks == durations.length) {
            durations = Arrays.copyOf(durations, durations.length * 2);
        }
        durations[ticks++] = durationNanos;
        playerSum += players;
        playerMax = Math.max(playerMax, players);
    }

    private void subscribeToGarbageCollections() {
        for (GarbageCollectorMXBean bean : ManagementFactory.getGarbageCollectorMXBeans()) {
            if (!(bean instanceof NotificationEmitter)) {
                continue;
            }
            final NotificationEmitter emitter = (NotificationEmitter) bean;
            final NotificationListener listener = new NotificationListener() {
                @Override
                public void handleNotification(Notification notification, Object handback) {
                    onGarbageCollection(notification);
                }
            };
            emitter.addNotificationListener(listener, null, null);
            gcUnsubscribers.add(new Runnable() {
                @Override
                public void run() {
                    try {
                        emitter.removeNotificationListener(listener);
                    } catch (ListenerNotFoundException ignored) {
                        // Already gone: nothing left to remove.
                    }
                }
            });
        }
    }

    /** JMX notification thread. */
    private void onGarbageCollection(Notification notification) {
        if (!GarbageCollectionNotificationInfo.GARBAGE_COLLECTION_NOTIFICATION.equals(notification.getType())) {
            return;
        }
        GarbageCollectionNotificationInfo info =
                GarbageCollectionNotificationInfo.from((CompositeData) notification.getUserData());
        GcInfo gc = info.getGcInfo();
        long before = 0;
        for (MemoryUsage usage : gc.getMemoryUsageBeforeGc().values()) {
            before += usage.getUsed();
        }
        long after = 0;
        for (MemoryUsage usage : gc.getMemoryUsageAfterGc().values()) {
            after += usage.getUsed();
        }
        long duration = gc.getDuration();
        gcCount.incrementAndGet();
        gcTotalMillis.addAndGet(duration);
        long max;
        do {
            max = gcMaxMillis.get();
        } while (duration > max && !gcMaxMillis.compareAndSet(max, duration));

        gcRows.add(System.currentTimeMillis() + "," + csv(info.getGcName()) + "," + csv(info.getGcAction()) + ","
                + csv(info.getGcCause()) + "," + duration + "," + before + "," + after);
    }

    private void drainLoop() {
        while (!stopping) {
            drain();
            try {
                Thread.sleep(WRITE_INTERVAL_MILLIS);
            } catch (InterruptedException e) {
                break;
            }
        }
        drain();
    }

    private void drain() {
        try {
            long[] row;
            while ((row = tickRows.poll()) != null) {
                StringBuilder line = new StringBuilder(96);
                for (int i = 0; i < row.length; i++) {
                    if (i > 0) {
                        line.append(',');
                    }
                    line.append(row[i]);
                    if (i == 3) {
                        // other_ns: the part of the tick spent outside the worlds.
                        line.append(',').append(Math.max(0L, row[2] - row[3]));
                    }
                }
                ticksOut.write(line.append('\n').toString());
            }
            String gc;
            while ((gc = gcRows.poll()) != null) {
                gcOut.write(gc);
                gcOut.write('\n');
            }
            ticksOut.flush();
            gcOut.flush();
        } catch (IOException e) {
            if (!writeFailed) {
                writeFailed = true;
                GammaEngine.LOGGER.error("Bench recording {} cannot write to {}", name, directory, e);
            }
            tickRows.clear();
            gcRows.clear();
        }
    }

    /** Stops the session, writes summary.json and returns a one-paragraph summary. */
    String finish() {
        for (Runnable unsubscribe : gcUnsubscribers) {
            unsubscribe.run();
        }
        stopping = true;
        writer.interrupt();
        try {
            writer.join(5000L);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        closeQuietly(ticksOut);
        closeQuietly(gcOut);

        double seconds = (System.nanoTime() - startNanos) / 1.0e9;
        TickStatistics.Mspt mspt = TickStatistics.Mspt.of(durations, ticks);
        double playersMean = ticks == 0 ? 0.0 : (double) playerSum / ticks;
        long tickNanos = 0;
        for (int i = 0; i < ticks; i++) {
            tickNanos += durations[i];
        }
        AttributionReport report = writeAttribution();
        String attributionJson = attribution == null ? "{\"enabled\": false}"
                : report == null ? "{\"enabled\": true, \"error\": \"export failed, see the server log\"}"
                : report.json(tickNanos);
        String json = summaryJson(seconds, mspt, playersMean, attributionJson);
        try {
            Writer out = new OutputStreamWriter(new FileOutputStream(new File(directory, "summary.json")),
                    StandardCharsets.UTF_8);
            try {
                out.write(json);
            } finally {
                out.close();
            }
        } catch (IOException e) {
            GammaEngine.LOGGER.error("Bench recording {} cannot write its summary", name, e);
        }

        StringBuilder text = new StringBuilder(256);
        text.append(String.format(Locale.ROOT, "%d ticks over %.1f s, TPS %.2f", ticks, seconds,
                seconds > 0 ? ticks / seconds : 0.0));
        if (mspt != null) {
            text.append(", MSPT ").append(mspt);
        }
        text.append(String.format(Locale.ROOT, ", players mean %.1f max %d", playersMean, playerMax));
        if (mspt != null && playersMean > 0) {
            text.append(String.format(Locale.ROOT, ", %.3f ms per player", mspt.mean() / playersMean));
        }
        text.append(String.format(Locale.ROOT, ", GC %d collection(s) %d ms total %d ms max",
                gcCount.get(), gcTotalMillis.get(), gcMaxMillis.get()));
        if (report != null) {
            text.append(", ").append(report.text(tickNanos));
        }
        text.append(". Files: ").append(directory.getPath());
        return text.toString();
    }

    /** Writes mods.csv and chunks.csv; {@code null} when the session does not attribute or the export failed. */
    private AttributionReport writeAttribution() {
        if (attribution == null) {
            return null;
        }
        try {
            AttributionReport report = AttributionReport.of(attribution, OwnerResolver.forServer(),
                    AttributionReport.CHUNK_ROWS);
            Writer mods = open("mods.csv");
            try {
                report.writeMods(mods);
            } finally {
                mods.close();
            }
            Writer chunks = open("chunks.csv");
            try {
                report.writeChunks(chunks);
            } finally {
                chunks.close();
            }
            return report;
        } catch (IOException | RuntimeException e) {
            GammaEngine.LOGGER.error("Bench recording {} cannot write its attribution", name, e);
            return null;
        }
    }

    private String summaryJson(double seconds, TickStatistics.Mspt mspt, double playersMean, String attributionJson) {
        SimpleDateFormat iso = new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.ROOT);
        iso.setTimeZone(TimeZone.getTimeZone("UTC"));
        StringBuilder json = new StringBuilder(1024);
        json.append("{\n");
        field(json, "name", quote(name));
        field(json, "server_version", quote(serverVersion()));
        field(json, "java_version", quote(System.getProperty("java.version")));
        field(json, "java_vm", quote(System.getProperty("java.vm.name") + " " + System.getProperty("java.vm.version")));
        StringBuilder args = new StringBuilder("[");
        for (String argument : ManagementFactory.getRuntimeMXBean().getInputArguments()) {
            args.append(args.length() > 1 ? ", " : "").append(quote(argument));
        }
        field(json, "jvm_args", args.append(']').toString());
        field(json, "start", quote(iso.format(new Date(startMillis))));
        field(json, "end", quote(iso.format(new Date())));
        field(json, "duration_s", number(seconds));
        field(json, "ticks", Integer.toString(ticks));
        field(json, "tps", number(seconds > 0 ? ticks / seconds : 0.0));
        if (mspt != null) {
            field(json, "mspt", "{\"mean\": " + number(mspt.mean()) + ", \"p50\": " + number(mspt.p50())
                    + ", \"p95\": " + number(mspt.p95()) + ", \"p99\": " + number(mspt.p99())
                    + ", \"max\": " + number(mspt.max()) + "}");
        } else {
            field(json, "mspt", "null");
        }
        field(json, "players", "{\"mean\": " + number(playersMean) + ", \"max\": " + playerMax + "}");
        field(json, "cost_per_player_ms", mspt != null && playersMean > 0 ? number(mspt.mean() / playersMean) : "null");
        field(json, "budget_per_player_ms", playersMean > 0 ? number(50.0 / playersMean) : "null");
        field(json, "attribution", attributionJson);
        json.append("  \"gc\": {\"count\": ").append(gcCount.get())
                .append(", \"total_ms\": ").append(gcTotalMillis.get())
                .append(", \"max_ms\": ").append(gcMaxMillis.get()).append("}\n");
        return json.append("}\n").toString();
    }

    private static String serverVersion() {
        try {
            return org.bukkit.Bukkit.getVersion();
        } catch (Throwable unavailable) {
            return "unknown";
        }
    }

    private static void field(StringBuilder json, String key, String value) {
        json.append("  ").append(quote(key)).append(": ").append(value).append(",\n");
    }

    private static String number(double value) {
        return String.format(Locale.ROOT, "%.4f", value);
    }

    static String quote(String value) {
        if (value == null) {
            return "null";
        }
        StringBuilder out = new StringBuilder(value.length() + 2).append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (c == '"' || c == '\\') {
                out.append('\\').append(c);
            } else if (c < 0x20) {
                out.append(String.format(Locale.ROOT, "\\u%04x", (int) c));
            } else {
                out.append(c);
            }
        }
        return out.append('"').toString();
    }

    /** Collector names, actions and causes contain spaces but never commas; quote them anyway. */
    static String csv(String value) {
        return value == null ? "" : '"' + value.replace("\"", "\"\"") + '"';
    }

    private static void closeQuietly(Writer out) {
        try {
            out.close();
        } catch (IOException ignored) {
            // The data is already flushed; a failed close loses nothing.
        }
    }
}
