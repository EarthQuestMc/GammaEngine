package io.github.gammaengine.world;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.platform.CpuTopology;
import net.minecraft.util.ChunkCoordinates;
import net.minecraft.world.WorldServer;
import net.minecraft.world.chunk.storage.AnvilChunkLoader;
import net.minecraft.world.gen.ChunkProviderServer;
import net.minecraftforge.common.chunkio.ChunkIOExecutor;

/**
 * Parallel prefetch of chunks that already exist on disk.
 *
 * <p>Startup spends most of its world time in one sequential loop that loads the spawn region chunk
 * by chunk: read the region file, inflate, decode NBT, commit into the world, and only then move to
 * the next chunk. The first three of those four steps touch nothing mutable in the world, which is
 * exactly the split Forge's own chunk IO executor already implements: stage one runs on a worker,
 * stage two commits on the server thread.
 *
 * <p>So the prefetcher does not invent a parallel loader. It queues the spawn region into that
 * executor before the sequential loop starts, widens the pool for the duration, and lets the
 * existing loop collect chunks whose reading and parsing are already done.
 *
 * <p>Two deliberate limits:
 * <ul>
 *   <li>only chunks that exist on disk are prefetched. A chunk that has to be generated stays on the
 *       sequential path, because world generation in 1.7.10, and above all modded world generation,
 *       is not thread safe.</li>
 *   <li>the commit into the world still happens on the server thread, in the same order as before,
 *       so chunk load events reach mods and plugins exactly as they did.</li>
 * </ul>
 */
public final class ChunkPrefetcher {
    private static final Runnable NO_OP = new Runnable() {
        @Override
        public void run() {
            // The sequential loop is what consumes the result; nothing to do on completion.
        }
    };

    private ChunkPrefetcher() {
    }

    /**
     * Queues every already-generated chunk of a square around the spawn point.
     *
     * @param world        world whose spawn region is about to be loaded
     * @param radiusBlocks half-width of the square, in blocks
     * @return how many chunks were queued for parallel reading
     */
    public static int prefetchSpawnRegion(WorldServer world, int radiusBlocks) {
        ChunkProviderServer provider = world.theChunkProviderServer;
        if (provider == null || !(provider.currentChunkLoader instanceof AnvilChunkLoader)) {
            return 0;
        }
        AnvilChunkLoader loader = (AnvilChunkLoader) provider.currentChunkLoader;
        ChunkCoordinates spawn = world.getSpawnPoint();
        int centerX = spawn.posX >> 4;
        int centerZ = spawn.posZ >> 4;
        int radiusChunks = radiusBlocks >> 4;

        widenPool();
        int queued = 0;
        try {
            for (int x = -radiusChunks; x <= radiusChunks; x++) {
                for (int z = -radiusChunks; z <= radiusChunks; z++) {
                    int chunkX = centerX + x;
                    int chunkZ = centerZ + z;
                    if (provider.chunkExists(chunkX, chunkZ)) {
                        continue; // already in memory
                    }
                    if (!loader.chunkExists(world, chunkX, chunkZ)) {
                        continue; // has to be generated, and generation stays single-threaded
                    }
                    ChunkIOExecutor.queueChunkLoad(world, loader, provider, chunkX, chunkZ, NO_OP);
                    queued++;
                }
            }
        } catch (Throwable error) {
            // Prefetching is an optimisation: if anything about it fails, the sequential loader
            // behind it still loads the world correctly.
            GammaEngine.LOGGER.warn("Chunk prefetch failed, falling back to sequential loading", error);
        }
        if (queued > 0) {
            GammaEngine.LOGGER.info("Prefetching {} spawn chunk(s) in parallel", queued);
        }
        return queued;
    }

    /**
     * Restores the chunk IO pool to the size it should have for the current player count. Called
     * once the spawn region is loaded.
     */
    public static void restorePool(int players) {
        ChunkIOExecutor.adjustPoolSize(players);
    }

    /**
     * Widens Forge's chunk IO pool for the duration of the prefetch.
     *
     * <p>The pool is normally sized from the player count, which is zero while the server boots, so
     * it would read the whole spawn region on a single thread. The executor only exposes the sizing
     * through a player count, hence the conversion here.
     */
    private static void widenPool() {
        int threads = Math.max(2, Math.min(8, CpuTopology.get().physicalCores()));
        ChunkIOExecutor.adjustPoolSize(threads * 50);
    }
}
