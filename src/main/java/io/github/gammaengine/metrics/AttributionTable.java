package io.github.gammaengine.metrics;

import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.IdentityHashMap;
import java.util.List;

/**
 * Tick time of one recording, summed by class and by chunk (level 2 of the bench probe).
 *
 * <p>Built for the hot path: no boxing, and no allocation once a class or a chunk has been seen.
 * Classes go through an {@link IdentityHashMap}, which stays small (one entry per ticking class);
 * chunks go through one fastutil {@code long -> slot} table per dimension, with the sums held in
 * parallel primitive arrays.
 *
 * <p>Thread invariant: {@link #record} runs on the server thread only, which owns every world
 * today. Everything else reads the table after the recording detached it and the server thread
 * passed a tick boundary (see {@link BenchRecorder#stop()}), so readers never race the writer.
 */
final class AttributionTable {
    /** Sums of one class. */
    static final class ClassCost {
        final Class<?> type;
        final boolean tile;
        long nanos;
        long ticks;

        ClassCost(Class<?> type, boolean tile) {
            this.type = type;
            this.tile = tile;
        }
    }

    /** Receives every chunk of the table, see {@link #forEachChunk}. */
    interface ChunkVisitor {
        void chunk(int dimension, int chunkX, int chunkZ, long nanos, long entityTicks, long tileTicks);
    }

    /** Chunk sums of one dimension: {@code slots} maps a packed chunk key to an index in the arrays. */
    private static final class Dimension {
        final int id;
        final Long2IntOpenHashMap slots = new Long2IntOpenHashMap(256);
        long[] keys = new long[256];
        long[] nanos = new long[256];
        long[] entityTicks = new long[256];
        long[] tileTicks = new long[256];
        int size;

        Dimension(int id) {
            this.id = id;
            slots.defaultReturnValue(-1);
        }

        void add(int chunkX, int chunkZ, boolean tile, long elapsed) {
            long key = chunkKey(chunkX, chunkZ);
            int slot = slots.get(key);
            if (slot < 0) {
                slot = size++;
                if (slot == keys.length) {
                    int grown = keys.length * 2;
                    keys = Arrays.copyOf(keys, grown);
                    nanos = Arrays.copyOf(nanos, grown);
                    entityTicks = Arrays.copyOf(entityTicks, grown);
                    tileTicks = Arrays.copyOf(tileTicks, grown);
                }
                keys[slot] = key;
                slots.put(key, slot);
            }
            nanos[slot] += elapsed;
            if (tile) {
                tileTicks[slot]++;
            } else {
                entityTicks[slot]++;
            }
        }
    }

    private final IdentityHashMap<Class<?>, ClassCost> classes = new IdentityHashMap<Class<?>, ClassCost>(256);
    private final Int2ObjectOpenHashMap<Dimension> dimensions = new Int2ObjectOpenHashMap<Dimension>();
    // Objects of one world are ticked in a row, so the last dimension almost always matches.
    private Dimension lastDimension;
    private long entityNanos;
    private long tileNanos;
    private long entityTicks;
    private long tileTicks;

    /** Server thread: charges one tick of an object of class {@code type} to its class and chunk. */
    void record(Class<?> type, boolean tile, int dimension, int chunkX, int chunkZ, long elapsed) {
        ClassCost cost = classes.get(type);
        if (cost == null) {
            cost = new ClassCost(type, tile);
            classes.put(type, cost);
        }
        cost.nanos += elapsed;
        cost.ticks++;

        Dimension chunks = lastDimension;
        if (chunks == null || chunks.id != dimension) {
            chunks = dimensions.get(dimension);
            if (chunks == null) {
                chunks = new Dimension(dimension);
                dimensions.put(dimension, chunks);
            }
            lastDimension = chunks;
        }
        chunks.add(chunkX, chunkZ, tile, elapsed);

        if (tile) {
            tileNanos += elapsed;
            tileTicks++;
        } else {
            entityNanos += elapsed;
            entityTicks++;
        }
    }

    Collection<ClassCost> classes() {
        return new ArrayList<ClassCost>(classes.values());
    }

    void forEachChunk(ChunkVisitor visitor) {
        List<Dimension> all = new ArrayList<Dimension>(dimensions.values());
        for (Dimension dimension : all) {
            for (int slot = 0; slot < dimension.size; slot++) {
                long key = dimension.keys[slot];
                visitor.chunk(dimension.id, chunkX(key), chunkZ(key), dimension.nanos[slot],
                        dimension.entityTicks[slot], dimension.tileTicks[slot]);
            }
        }
    }

    int chunkCount() {
        int count = 0;
        for (Dimension dimension : dimensions.values()) {
            count += dimension.size;
        }
        return count;
    }

    long entityNanos() {
        return entityNanos;
    }

    long tileNanos() {
        return tileNanos;
    }

    long entityTicks() {
        return entityTicks;
    }

    long tileTicks() {
        return tileTicks;
    }

    static long chunkKey(int chunkX, int chunkZ) {
        return ((long) chunkX << 32) | (chunkZ & 0xFFFFFFFFL);
    }

    static int chunkX(long key) {
        return (int) (key >> 32);
    }

    static int chunkZ(long key) {
        return (int) key;
    }
}
