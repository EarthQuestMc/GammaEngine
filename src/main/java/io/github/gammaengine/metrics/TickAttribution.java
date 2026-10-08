package io.github.gammaengine.metrics;

import io.github.gammaengine.GammaEngine;
import net.minecraft.entity.Entity;
import net.minecraft.tileentity.TileEntity;
import net.minecraft.util.MathHelper;
import net.minecraft.world.World;

/**
 * Level 2 of the bench probe: the time of every entity and tile entity tick, charged to the class
 * of the object and to its chunk. {@code World.updateEntities} calls {@link #start()} before each
 * tick and {@link #entity} or {@link #tileEntity} after it.
 *
 * <p>Cost while off, the default: {@link #start()} reads one static field and returns 0, and the
 * second hook returns on that 0. No {@code nanoTime}, no allocation. While a recording with
 * attribution runs: two {@code nanoTime} calls per ticked object, one identity lookup by class, one
 * primitive hash lookup by chunk, no boxing. An object whose tick throws is not charged: the hook
 * after the tick is skipped and the world handles the exception exactly as before.
 *
 * <p>Thread invariant: the hooks run on the server thread, the only thread that ticks worlds today,
 * and it is the only writer of the attached table. {@link #attach} and {@link #detach} publish the
 * table through the volatile field; a hook that read the table before a detach may still write to
 * it until the end of that object's tick, which is why {@link BenchRecorder#stop()} waits for the
 * next tick before reading the table from another thread.
 */
public final class TickAttribution {
    private static volatile AttributionTable active;

    private TickAttribution() {
    }

    /** Server thread, before an object ticks: the start time, or 0 while attribution is off. */
    public static long start() {
        return active != null ? System.nanoTime() : 0L;
    }

    /** Server thread, after {@code world.updateEntity(entity)} returned normally. */
    public static void entity(World world, Entity entity, long start) {
        if (start != 0L) {
            long elapsed = System.nanoTime() - start;
            charge(entity.getClass(), false, world, MathHelper.floor_double(entity.posX) >> 4,
                    MathHelper.floor_double(entity.posZ) >> 4, elapsed);
        }
    }

    /** Server thread, after {@code tile.updateEntity()} returned normally. */
    public static void tileEntity(World world, TileEntity tile, long start) {
        if (start != 0L) {
            long elapsed = System.nanoTime() - start;
            charge(tile.getClass(), true, world, tile.xCoord >> 4, tile.zCoord >> 4, elapsed);
        }
    }

    private static void charge(Class<?> type, boolean tile, World world, int chunkX, int chunkZ, long elapsed) {
        AttributionTable table = active;
        if (table == null) {
            return; // detached while this object ticked
        }
        try {
            table.record(type, tile, world.provider.dimensionId, chunkX, chunkZ, elapsed);
        } catch (RuntimeException e) {
            // The probe must never be the reason a tick fails: the world would report it as a
            // crash of the entity. Stop attributing and say why.
            active = null;
            GammaEngine.LOGGER.error("Tick attribution failed and was turned off for this recording", e);
        }
    }

    /** Starts charging ticks to {@code table}. */
    static void attach(AttributionTable table) {
        active = table;
    }

    /** Stops charging ticks; see the class comment for when the table can be read. */
    static void detach() {
        active = null;
    }

    static boolean isActive() {
        return active != null;
    }
}
