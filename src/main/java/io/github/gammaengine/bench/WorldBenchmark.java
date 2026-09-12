package io.github.gammaengine.bench;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.autothread.AutoThreadRuntime;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.LatencyHistogram;
import net.minecraft.block.Block;
import net.minecraft.entity.Entity;
import net.minecraft.entity.passive.EntityPig;
import net.minecraft.init.Blocks;
import net.minecraft.server.MinecraftServer;
import net.minecraft.util.ChunkCoordinates;
import net.minecraft.world.WorldServer;

import java.io.File;
import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;

/**
 * Reproducible synthetic load, used to make phase 1 measurable.
 *
 * <p>An empty 1.7.10 world ticks in about 0.2 ms, which is below the noise floor of any optimisation
 * worth making. This harness builds a load the server can be measured under: a fixed square of
 * chunks held loaded, a fixed number of entities with real AI, and a fixed number of ticking tile
 * entities, all placed from a fixed seed so two runs are comparable.
 *
 * <p>It runs entirely on the server thread, drives itself from the runtime's per-tick hook, and puts
 * the world back as it found it when the run ends: entities are removed and placed blocks are
 * restored. A benchmark that leaves debris behind cannot be run twice.
 *
 * <p>What it reports is deliberately wider than tick time: CPU time actually consumed, garbage
 * collection, and heap use, because "consume less" is the goal and tick time alone hides all three.
 */
public final class WorldBenchmark {
    private static final WorldBenchmark INSTANCE = new WorldBenchmark();
    private static final int WARMUP_TICKS = 100;

    private final Runnable tickTask = new Runnable() {
        @Override
        public void run() {
            onTick();
        }
    };

    private volatile boolean running;
    private BenchmarkSpec spec;
    private WorldServer world;
    private int[] chunkX;
    private int[] chunkZ;
    private final List<Entity> spawned = new ArrayList<Entity>();
    private final List<int[]> placedBlocks = new ArrayList<int[]>();
    private int warmupLeft;
    private int ticksLeft;
    private long startNanos;
    private long startCpuNanos;
    private long startGcCount;
    private long startGcMillis;
    private long startHeapUsed;
    private Listener listener;

    private WorldBenchmark() {
    }

    public static WorldBenchmark get() {
        return INSTANCE;
    }

    /** Receives progress and the final report. Implemented by the command that started the run. */
    public interface Listener {
        void message(String line);
    }

    public boolean isRunning() {
        return running;
    }

    /**
     * Starts a run. Must be called from the server thread.
     *
     * @return an error message, or {@code null} when the run started
     */
    public synchronized String start(BenchmarkSpec spec, Listener listener) {
        if (running) {
            return "A benchmark is already running.";
        }
        MinecraftServer server = MinecraftServer.getServer();
        if (server == null || server.worldServers == null || server.worldServers.length == 0) {
            return "No world is loaded.";
        }
        this.spec = spec;
        this.listener = listener;
        this.world = server.worldServers[0];

        say("Benchmark setup: " + spec);
        setUpChunks();
        setUpTileEntities();
        setUpEntities();
        say("Setup done: " + chunkX.length + " chunks, " + spawned.size() + " entities, "
                + placedBlocks.size() + " tile entities. Warming up for " + WARMUP_TICKS + " ticks.");

        warmupLeft = WARMUP_TICKS;
        ticksLeft = spec.ticks();
        running = true;
        AutoThreadRuntime.get().addTickTask(tickTask);
        return null;
    }

    /** Aborts a run and restores the world. */
    public synchronized void cancel() {
        if (!running) {
            return;
        }
        finish(true);
    }

    private void setUpChunks() {
        ChunkCoordinates spawn = world.getSpawnPoint();
        int centerX = spawn.posX >> 4;
        int centerZ = spawn.posZ >> 4;
        int radius = spec.chunkRadius();
        int side = radius * 2 + 1;
        chunkX = new int[side * side];
        chunkZ = new int[side * side];
        int index = 0;
        for (int x = -radius; x <= radius; x++) {
            for (int z = -radius; z <= radius; z++) {
                chunkX[index] = centerX + x;
                chunkZ[index] = centerZ + z;
                world.theChunkProviderServer.loadChunk(chunkX[index], chunkZ[index]);
                index++;
            }
        }
    }

    /**
     * Places hoppers, which tick every few ticks and look at their neighbours, so they exercise the
     * tile entity path rather than just sitting in the list.
     */
    private void setUpTileEntities() {
        Random random = new Random(0xBEEFL);
        int radius = spec.chunkRadius() * 16;
        ChunkCoordinates spawn = world.getSpawnPoint();
        Block hopper = Blocks.hopper;
        if (hopper == null) {
            return;
        }
        for (int i = 0; i < spec.tileEntities(); i++) {
            int x = spawn.posX + random.nextInt(radius * 2 + 1) - radius;
            int z = spawn.posZ + random.nextInt(radius * 2 + 1) - radius;
            int y = world.getHeightValue(x, z);
            if (y < 2 || y > 250) {
                continue;
            }
            if (!world.isAirBlock(x, y, z)) {
                continue;
            }
            if (world.setBlock(x, y, z, hopper)) {
                placedBlocks.add(new int[]{x, y, z});
            }
        }
    }

    /**
     * Spawns pigs: passive, so they neither burn in daylight nor despawn without players, and they
     * run the full living-entity tick including AI and pathfinding.
     */
    private void setUpEntities() {
        Random random = new Random(0xCAFEL);
        int radius = spec.chunkRadius() * 16;
        ChunkCoordinates spawn = world.getSpawnPoint();
        for (int i = 0; i < spec.entities(); i++) {
            int x = spawn.posX + random.nextInt(radius * 2 + 1) - radius;
            int z = spawn.posZ + random.nextInt(radius * 2 + 1) - radius;
            int y = world.getHeightValue(x, z);
            if (y < 2 || y > 250) {
                continue;
            }
            EntityPig pig = new EntityPig(world);
            pig.setLocationAndAngles(x + 0.5D, y, z + 0.5D, random.nextFloat() * 360.0F, 0.0F);
            pig.func_110163_bv(); // persistence: never despawned by the engine
            if (world.spawnEntityInWorld(pig)) {
                spawned.add(pig);
            }
        }
    }

    private void onTick() {
        if (!running) {
            return;
        }
        // Keep the area loaded: with no player online nothing else holds these chunks, and a
        // benchmark whose world unloads halfway measures nothing.
        for (int i = 0; i < chunkX.length; i++) {
            world.theChunkProviderServer.loadChunk(chunkX[i], chunkZ[i]);
        }

        if (warmupLeft > 0) {
            warmupLeft--;
            if (warmupLeft == 0) {
                beginMeasurement();
            }
            return;
        }

        ticksLeft--;
        if (ticksLeft <= 0) {
            finish(false);
        }
    }

    private void beginMeasurement() {
        GammaProfiler.get().startSession();
        startNanos = System.nanoTime();
        startCpuNanos = processCpuNanos();
        startGcCount = gcCount();
        startGcMillis = gcMillis();
        startHeapUsed = heapUsed();
        say("Measuring for " + spec.ticks() + " ticks.");
    }

    private void finish(boolean aborted) {
        running = false;
        AutoThreadRuntime.get().removeTickTask(tickTask);

        String report = aborted ? null : buildReport();
        GammaProfiler.get().stopSession();
        tearDown();

        if (report != null) {
            for (String line : report.split("\n")) {
                say(line);
            }
            File file = GammaProfiler.get().writeReport(report);
            if (file != null) {
                say("Report written to " + file.getPath());
            }
        } else {
            say("Benchmark cancelled, world restored.");
        }
        listener = null;
    }

    private String buildReport() {
        long elapsedNanos = Math.max(1L, System.nanoTime() - startNanos);
        long cpuNanos = Math.max(0L, processCpuNanos() - startCpuNanos);
        int ticks = spec.ticks();

        LatencyHistogram.Snapshot tick = GammaProfiler.get().registry().histogram("server.tick").snapshot();

        StringBuilder out = new StringBuilder(2048);
        out.append("=== GammaEngine benchmark: ").append(spec.name()).append(" ===\n");
        out.append("Load: ").append(spec).append('\n');
        out.append(String.format("Duration: %d ticks in %.2f s (%.2f TPS)%n",
                ticks, elapsedNanos / 1.0e9, ticks / (elapsedNanos / 1.0e9)));
        out.append(String.format("MSPT: mean=%.3f p50=%.3f p95=%.3f p99=%.3f max=%.3f ms%n",
                tick.meanMillis(), tick.p50Millis(), tick.p95Millis(), tick.p99Millis(), tick.maxMillis()));

        if (cpuNanos > 0) {
            double cores = (double) cpuNanos / elapsedNanos;
            out.append(String.format("CPU: %.1f ms total, %.2f core(s) average, %.3f core-ms per tick%n",
                    cpuNanos / 1.0e6, cores, cpuNanos / 1.0e6 / ticks));
        }
        out.append(String.format("GC: %d collection(s), %d ms%n",
                gcCount() - startGcCount, gcMillis() - startGcMillis));
        out.append(String.format("Heap used: %.1f MiB at start, %.1f MiB at end%n",
                startHeapUsed / 1048576.0, heapUsed() / 1048576.0));
        out.append(String.format("World: %d chunks loaded, %d entities, %d tile entities%n",
                world.theChunkProviderServer.getLoadedChunkCount(),
                world.loadedEntityList.size(), world.loadedTileEntityList.size()));

        out.append('\n').append(GammaProfiler.get().buildReport());
        return out.toString();
    }

    /** Removes everything the benchmark added, so the world is reusable for the next run. */
    private void tearDown() {
        for (Entity entity : spawned) {
            if (entity != null && !entity.isDead) {
                entity.setDead();
            }
        }
        spawned.clear();

        for (int[] position : placedBlocks) {
            world.setBlockToAir(position[0], position[1], position[2]);
        }
        placedBlocks.clear();
        spec = null;
        world = null;
        chunkX = null;
        chunkZ = null;
    }

    private void say(String line) {
        GammaEngine.LOGGER.info("[bench] {}", line);
        Listener target = listener;
        if (target != null) {
            target.message(line);
        }
    }

    private static long processCpuNanos() {
        try {
            java.lang.management.OperatingSystemMXBean os = ManagementFactory.getOperatingSystemMXBean();
            if (os instanceof com.sun.management.OperatingSystemMXBean) {
                return ((com.sun.management.OperatingSystemMXBean) os).getProcessCpuTime();
            }
        } catch (Throwable ignored) {
            // Not a HotSpot-compatible JVM: CPU time is simply not reported.
        }
        return 0L;
    }

    private static long gcCount() {
        long total = 0;
        for (GarbageCollectorMXBean bean : ManagementFactory.getGarbageCollectorMXBeans()) {
            long count = bean.getCollectionCount();
            if (count > 0) {
                total += count;
            }
        }
        return total;
    }

    private static long gcMillis() {
        long total = 0;
        for (GarbageCollectorMXBean bean : ManagementFactory.getGarbageCollectorMXBeans()) {
            long millis = bean.getCollectionTime();
            if (millis > 0) {
                total += millis;
            }
        }
        return total;
    }

    private static long heapUsed() {
        return ManagementFactory.getMemoryMXBean().getHeapMemoryUsage().getUsed();
    }

}
