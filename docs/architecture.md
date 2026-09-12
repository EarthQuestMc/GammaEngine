# GammaEngine architecture

GammaEngine adds one subsystem to Crucible: the **AutoThread runtime**, which owns every threading
decision the server makes. Nothing else in the server decides what runs in parallel, and no mod,
plugin or administrator declares anything about thread safety. The runtime observes what code
actually touches at runtime and picks the safest schedule that still uses the machine.

The code lives under `src/main/java/io/github/gammaengine`. Patches to Minecraft, Forge and Bukkit
classes stay one-liners that call into it, so the fork can still be rebased onto upstream Crucible.

## Where the server spends a tick today

These are the paths the runtime has to take over, verified in the generated workspace
(`eclipse/cauldron/src/main/java`, line numbers from the 1.7.10 / Forge 1614 workspace):

| Stage | Location |
| --- | --- |
| Server loop, sleep and catch-up | `net/minecraft/server/MinecraftServer.java:642` (`run`) |
| Server tick | `MinecraftServer.java:830` (`tick`) |
| Forge pre/post server tick | inside `tick`, via `FMLCommonHandler.onPre/onPostServerTick` |
| Bukkit scheduler heartbeat | `MinecraftServer.java:910` (`updateTimeLightAndEntities`) |
| Forge chunk IO drain | same method, `ChunkIOExecutor.tick()` |
| Per-world tick | `MinecraftServer.updateTimeLightAndEntities` → `WorldServer.tick()` at `world/WorldServer.java:224` |
| Mob spawning, chunk unload, pending block ticks, random block ticks, chunk map, villages | inside `WorldServer.tick` |
| Per-world entity and tile entity tick | `WorldServer.updateEntities()` at `world/WorldServer.java:655` → `World.updateEntities()` at `world/World.java:2350` |
| Entity tick | `World.updateEntity(Entity)` at `world/World.java:2660` |
| Tile entity tick | loop inside `World.updateEntities` calling `TileEntity.updateEntity()` |
| Forge event dispatch | `cpw/mods/fml/common/eventhandler/EventBus.java:150` (`post`) |
| Bukkit scheduler | `src/main/java/org/bukkit/craftbukkit/v1_7_R4/scheduler/CraftScheduler.java` |
| Chunk provider | `world/gen/ChunkProviderServer.java` (`loadChunk`, `provideChunk`, `unloadQueuedChunks`) |
| Chunk serialization | `world/chunk/storage/AnvilChunkLoader.java` (`loadChunk__Async`, `saveChunk`, `writeChunkToNBT`, `readChunkFromNBT`) |
| Region file IO | `world/chunk/storage/RegionFile.java` (synchronized read and write) |
| Existing async chunk loading | `net/minecraftforge/common/chunkio/ChunkIOExecutor.java` |

Everything above runs on one thread today: the Minecraft server thread. The only pre-existing
parallelism is Forge's chunk IO executor, Netty, and Bukkit's async scheduler tasks.

## Subsystems

Implemented so far (phase 0 and 1):

| Class | Responsibility |
| --- | --- |
| `GammaEngine` | Fork identity and shared logger |
| `autothread.AutoThreadRuntime` | Lifecycle, main-thread identity, tick instrumentation, status |
| `config.GammaConfig` | `GammaAutoThread.yml`: resource limits and diagnostics only |
| `platform.CpuTopology` | Physical cores vs SMT siblings, NUMA package count |
| `concurrent.ThreadPools` | The pools and the rules that size them |
| `concurrent.ManagedPool` | Instrumented, resizable pool: queue wait, execution time, depth |
| `profiler.GammaProfiler` | Metric registry, profiling sessions, reports |
| `profiler.LatencyHistogram` | p50/p95/p99 with bounded memory and lock-free recording |
| `profiler.TickStatistics` | Rolling TPS and exact MSPT percentiles over the last minute |
| `nativeengine.NativeEngine` | Single JNI façade, always with a Java fallback |
| `command.AutoThreadCommand` | `/autothread status|workers|regions|conflicts|native|profile` |

Planned, in build order (see `docs/threading-model.md` for the rules each of these must obey):

`RegionManager`, `Region`, `RegionOwnership`, `RegionContext`, `RegionScheduler`,
`EntityScheduler`, `AccessTracker`, `ConflictDetector`, `DependencyGraph`,
`CrossRegionCoordinator`, `MultiRegionTransaction`, `ChunkSaveSnapshot`, `AsyncChunkLoader`,
`AsyncChunkSaver`, `AutoQuarantineManager`, `RuntimeProfiler` learning layer.

## Thread pools

There is no single engine executor. Work of different shapes must not share a queue, because a
two-second chunk compression queued in front of a region tick adds two seconds to that region's
tick time. Sizes are derived from **physical** cores.

| Pool | Default size (8c/16t reference machine) | Work |
| --- | --- | --- |
| `RegionTick` | cores − 1 = 7 | World simulation. With the server thread, saturates the 8 cores |
| `ChunkIO` | cores / 2 = 4 | Disk reads and writes, blocking |
| `ChunkWorker` | cores / 2 = 4 | NBT encode/decode, compression, generation helpers |
| `Async` | cores / 2 = 4 | Plugin async tasks and engine background work |
| `Native` | cores / 4 = 2 | Batched JNI work |
| `Background` | 1 | Metric aggregation, report writing |

Workers run one priority below the server thread so the coordinator always wins a contended
scheduling decision, and every pool thread is a daemon so a failed boot cannot hang the process.

## Boot sequence

1. `MinecraftServer.run()` calls `AutoThreadRuntime.boot()` before any world exists: configuration,
   CPU topology, pools, optional native library.
2. `startServer()` runs unchanged: Forge loads mods, Bukkit loads plugins, worlds load.
3. `AutoThreadRuntime.onServerStarted()` runs after `handleServerStarted`.
4. Every tick calls `onTickStart()` / `onTickEnd()` for metrics.
5. `MinecraftServer.stopServer()` calls `shutdown()` after worlds are saved, never before: workers
   must finish their queued writes while the world data is still consistent.
