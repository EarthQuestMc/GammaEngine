# GammaEngine roadmap

This is the working plan and the progress tracker. It says what each phase does, how it is carried
out, how we decide it is finished, and where the project stands right now.

## Where we are

| Phase | Subject | State |
| --- | --- | --- |
| 0 | Build, baseline, metrics | **Done** |
| 1 | Optimise existing paths, threading model untouched | **In progress** |
| 2 | Chunk snapshots, async load and save | Not started |
| 3 | Regions: ownership and context, tracing only | Not started |
| 4 | Access tracking, conflict detection, dependency graph | Not started |
| 5 | Region scheduler, first parallel ticks | Not started |
| 6 | Entity scheduler, cross-region transactions | Not started |
| 7 | Bytecode instrumentation of mods and plugins | Not started |
| 8 | Auto-quarantine and auto-learning | Not started |
| 9 | Native Rust library | **Priority 1 done**, priority 2 pending |
| 10 | Pathfinding, lighting, collisions, modded networks | Not started |

Phase 9 was started early, out of order, because the compression and hashing work it contains has
no dependency on the threading model and because the measurement harness it needs was already
built in phase 0.

## How this plan was built

Three constraints shaped the order.

**Measure before changing.** A multithreaded server is a machine for producing heisenbugs. Without
a baseline and per-subsystem metrics, every later change is an opinion. So phase 0 builds the
instruments, and every phase after it must show its numbers.

**Correctness infrastructure before parallelism.** Ownership, access tracking and conflict
detection are useless on their own, but running them for a while in a single-threaded server is how
we learn what mods really touch, with zero risk. Phases 3 and 4 therefore ship *before* anything
runs in parallel, and they ship switched on. When phase 5 finally ticks two regions at once, the
detector that would catch the mistake is already in production and already trusted.

**Cheapest reversible work first.** Phases 1 and 2 give real gains without touching the threading
model, which means they can be validated with ordinary testing and reverted independently. The
risky work comes after, on a codebase that is already faster and already instrumented.

Inside every phase the method is the same: read the code, write down the invariant that must hold,
write or adapt the test, implement, compile, test, benchmark against the baseline, keep or revert.
No phase is finished until the server boots, ticks and stops cleanly with the change in place.

---

## Phase 0 — Build, baseline, metrics

**Done.**

Goal: be able to build the untouched server, run it, and measure it, before changing any behaviour.

How it proceeded:

1. Identified the exact versions in use: Minecraft 1.7.10, MCP 9.08, Forge 10.13.4.1614, Bukkit
   1.7.10-R0.1-SNAPSHOT, Gradle 8.0, build JDK 8 with a Java 17 toolchain for `buildSrc`.
2. Created the patched workspace with `setupCrucible` and built the unmodified server, to have a
   reference artifact.
3. Mapped the tick paths that later phases take over, and recorded them in
   [architecture.md](architecture.md): server loop, server tick, per-world tick, entity tick, tile
   entity tick, Forge event bus, Bukkit scheduler, chunk provider, chunk loader, region file.
4. Built the measurement layer: latency histograms with p50/p95/p99, a metric registry, rolling TPS
   and exact MSPT percentiles, profiling sessions that write a report file.
5. Hooked the runtime into `MinecraftServer` at four points only: boot, server started, tick
   start/end, shutdown.
6. Booted a real server and recorded the baseline.

Baseline, empty world, no mods, development machine:

| Metric | Value |
| --- | --- |
| TPS | 19.94 |
| MSPT mean | 0.22 ms |
| MSPT p99 | 0.62 ms |
| Boot, world creation | 4.0 s |
| Boot, existing world | 1.1 s |

Exit criteria, all met: reproducible build documented, server boots and stops cleanly, `/autothread`
reports live numbers, unit tests green.

---

## Phase 1 — Optimise existing paths

**In progress.** The threading model is not touched in this phase: one simulation thread, same
order of operations, same results.

Goal: make the single-threaded server measurably faster, so that later parallel work starts from a
clean base and so that the gains from parallelism can be told apart from the gains from ordinary
optimisation.

How it proceeds:

1. Profile a loaded server and rank the hot paths by total cost, using the phase 0 report.
2. Chunk IO first: today `AnvilChunkLoader.saveChunk` serialises and compresses on the calling
   thread, and `RegionFile` synchronises around every read and write. Compression moves to the
   worker pool, with the native library when present.
3. Allocation pressure in the tick loops: temporary lists and maps in the entity and tile entity
   loops, boxed coordinates, per-tick NBT objects. Replace only what the profiler shows, with
   primitive collections that already ship with the server (fastutil, koloboke).
4. Collection choices on hot structures: the entity and tile entity lists are scanned and mutated
   every tick.
5. Network: move packet compression off the tick, reuse buffers where it is provably safe, never
   change the protocol.

Method for each item: measure before, change, measure after, keep only if the gain is real and the
behaviour is identical.

Exit criteria: a documented before/after table per optimisation, no behaviour change, tests green,
a world that is byte-identical after the same scenario.

---

## Phase 2 — Chunk snapshots, async load and save

Goal: get disk work, compression and NBT off the simulation thread without ever serialising a
chunk that another thread can mutate.

How it will proceed:

1. `ChunkSaveSnapshot`: an immutable, compact copy of everything a chunk save needs, taken on the
   owning thread in the shortest possible window.
2. `AsyncChunkSaver`: snapshot on the tick, then serialise, compress and write on the IO pool.
3. `AsyncChunkLoader`: read, decompress and decode NBT off-thread; only the final commit into the
   world happens on the owning thread.
4. Keep Forge's existing `ChunkIOExecutor` working throughout; it is replaced, not bypassed.

Exit criteria: worlds saved by the new path are readable by vanilla and by upstream Crucible; a
crash during heavy save leaves a loadable world; measured reduction of the save spike in MSPT p99.

---

## Phase 3 — Regions: ownership and context

Goal: give every chunk, entity and tile entity a single logical owner, and make the current owner
knowable from any thread. Nothing runs in parallel in this phase.

How it will proceed:

1. `Region`, `RegionOwnership`, `RegionManager`: group loaded chunks into regions that do not
   interact, with merge and split as the world changes.
2. `RegionContext`: the "which region am I allowed to write" answer, available to any code that
   asks, cheap enough to consult on hot paths.
3. Regions are created and maintained, ownership is tracked, and the server still ticks everything
   on one thread. The only visible effect is `/autothread regions`.

Exit criteria: ownership is correct and stable for a day of real play, region merges and splits do
not lose objects, no measurable cost added to the tick.

---

## Phase 4 — Access tracking, conflicts, dependency graph

Goal: learn what the loaded mods really touch, while it is still impossible to corrupt anything.

How it will proceed:

1. `AccessTracker`: records accesses by type (region-local read/write, global, cross-region, async
   compute), attributed to the calling mod or plugin.
2. `ConflictDetector`: given the tracked accesses, reports what *would* have conflicted if the
   regions had been ticked in parallel. In this phase it only reports.
3. `DependencyGraph`: builds the real relationships between chunks, tile entities, inventories and
   modded networks, and updates as the world changes.
4. Aggregated reporting, because a busy server produces hundreds of thousands of identical events.

Exit criteria: a modded server runs for hours with tracking on, the conflict report is stable and
explainable, and the overhead is small enough to leave enabled.

---

## Phase 5 — Region scheduler, first parallel ticks

Goal: tick a small number of provably independent regions at the same time.

How it will proceed:

1. `RegionScheduler` on the region tick pool, starting with two regions and a hard serialisation
   fallback.
2. The phase 4 detector stays on, now as a guard: a conflict lowers parallelism immediately.
3. Widen gradually, scenario by scenario, never faster than the evidence.

Exit criteria: determinism runs of the same scenario produce identical worlds, no duplication, no
lost update, and a real gain in MSPT under load.

---

## Phase 6 — Entity scheduler, cross-region transactions

Goal: handle everything that legitimately crosses a region boundary.

How it will proceed:

1. `EntityScheduler`: atomic migration of an entity from one region to another, never ticked twice,
   never lost, covering players, mobs, items, projectiles, vehicles and modded entities.
2. `CrossRegionCoordinator` and `MultiRegionTransaction`: determine the regions involved, lock them
   in ascending id order so a cycle is impossible, execute, release, with timeout and metrics.
3. Cover the real cases: pipes and item transfer, energy and fluid networks, teleports, explosions,
   multiblocks.

Exit criteria: no deadlock under stress, no item duplication across a boundary, transactions
visible and measured in `/autothread conflicts`.

---

## Phase 7 — Instrumentation of mods and plugins

Goal: extend tracking and protection to code we do not own, without asking authors for anything.

How it will proceed:

1. An ASM transformer, loaded through the existing LaunchWrapper/Forge coremod path, targeting only
   the sensitive access points: world and chunk mutation, entity and tile entity collections,
   inventories, the Forge event bus, the Bukkit scheduler, known global singletons.
2. Instrument narrowly. Every instrumented site must justify its cost.
3. Raise parallelism as the instrumented evidence accumulates.

Exit criteria: a large modpack boots with instrumentation on, the startup cost is acceptable, and
the tracked data is richer than what phase 4 could see.

---

## Phase 8 — Auto-quarantine and auto-learning

Goal: make the runtime improve itself and protect itself without an administrator in the loop.

How it will proceed:

1. `AutoQuarantineManager`: on repeated conflicts, threading exceptions, stalls or detected
   non-determinism, lower parallelism for the smallest responsible unit: an object, a tile entity
   type, a method, an event listener, a geographic area. Never a whole mod.
2. Learning: persist observed behaviour per class and method, invalidated when the mod jar hash,
   the server version or the instrumentation changes.
3. Periodic re-testing, so a component quarantined by one bad interaction can earn its parallelism
   back.

Exit criteria: the profile survives a restart, a deliberately broken test mod gets quarantined at
the right granularity, and the rest of that mod keeps running in parallel.

---

## Phase 9 — Native Rust library

**Priority 1 done.** `native/` builds a Rust `cdylib`, the server loads it when present and falls
back to Java when it is absent, refuses it on ABI mismatch, and catches panics at the boundary.

Implemented and measured on 200 kB of chunk-shaped data, JNI included:

| Operation | Java | Rust | Change |
| --- | --- | --- | --- |
| zlib compress | 195 MiB/s | 265 MiB/s | +36% |
| zlib decompress | 1010 MiB/s | 1544 MiB/s | +53% |
| XXH64 | 4296 MiB/s | 7509 MiB/s | +75% |
| Compressed size | 19 268 B | 18 806 B | −2.4% |

Region-file sector arithmetic and location-table validation are implemented as pure functions with
tests, ready for the phase 2 writer.

Priority 2, once the Java architecture is stable: NBT binary encode and decode, pathfinding over
immutable snapshots, batch collision and spatial queries. Each one has to beat Java on the existing
harness or it does not ship. Details in [native-engine.md](native-engine.md).

---

## Phase 10 — Heavy paths

Goal: move the remaining expensive work off the region thread once the architecture can support it.

Planned: asynchronous lighting (snapshot, worker computes, owner applies), pathfinding on
snapshots, batched collisions, and dedicated adapters for the big modded networks (AE2, BuildCraft,
IC2, Thermal Expansion, Ender IO, Mekanism, GregTech, Railcraft, ComputerCraft, OpenComputers). The
generic mechanism must work without those adapters; adapters only make known cases faster.

---

## Cross-cutting work, running in every phase

**Benchmarks.** A reproducible harness and the eight scenarios: players at spawn, players spread
out, large industrial bases, exploration and chunk generation, many mobs, many tile entities, large
networks, mass world edits. Player counts from 50 to 500. Target: 250 players on a large modpack at
20 TPS with p95 MSPT at or under 50 ms, on the AMD EPYC 4344P reference machine.

**Save integrity and determinism.** A world must stay readable after a controlled crash, a stop
under load, repeated saves, region migrations, cross-region transactions and chunk reload. Repeated
runs of one scenario must produce the same world, checked by hashing decompressed chunk content.

**CI.** Java build, Rust build, unit tests, integration tests, a server boot test and a benchmark
smoke test, on Linux x86_64 first and Windows x86_64 second.

**Compatibility bench.** IC2, BuildCraft, AE2, Thermal Expansion, Thaumcraft, Ender IO, Mekanism,
GregTech, Galacticraft, Twilight Forest, Railcraft, MineFactory Reloaded, ComputerCraft,
OpenComputers, plus WorldEdit, WorldGuard, Vault, PermissionsEx, an Essentials equivalent and a
1.7.10-compatible ProtocolLib.
