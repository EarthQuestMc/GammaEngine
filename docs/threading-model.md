# GammaEngine threading model

This document is the contract. Any patch, subsystem or optimisation that breaks a rule below is
wrong, even when it makes a benchmark faster.

## Priority order

When two goals conflict, the earlier one wins:

1. **World integrity.** No lost update, no duplicated item, no desynchronised inventory.
2. **No silent corruption.** A failure must be loud. An optimisation that can corrupt quietly is
   removed, not documented.
3. **Compatibility.** Forge mods, Bukkit plugins, existing worlds, existing save formats and the
   1.7.10 protocol keep working, unmodified.
4. **Stability.** No deadlock, no unbounded stall, no crash a single thread would not have had.
5. **Performance.** Last, and only within the four rules above.

## Ownership

* Every loaded chunk has exactly one **owning region** at any instant.
* Every entity has exactly one owning region, which is the region owning the chunk it is in.
* Every tile entity belongs to the region owning its chunk.
* A mutable world object is only ever written by the thread currently executing its owner.
* Ownership changes are atomic and happen between ticks of the objects involved, never during.

The main server thread owns everything not yet assigned to a region, and stays the owner of truly
global state (the world clock, the player list, Forge's global registries).

## The unknown-access rule

The runtime starts pessimistic and earns parallelism:

```
unknown access -> safe scheduling -> observe -> optimize later
```

A class, method, tile entity type or event listener the runtime has never seen runs serialized. It
becomes eligible for parallel execution only after it has been observed doing nothing dangerous,
repeatedly. The reverse is immediate: one conflict, one threading exception or one detected
non-deterministic behaviour and the runtime lowers that component's parallelism again, at the
finest granularity it can (a single object, a class, a method), never by disabling a whole mod.

## Access classification

| Type | Meaning | Default policy |
| --- | --- | --- |
| `READ` | Reads data owned by the current region | Parallel |
| `WRITE` | Writes data owned by the current region | Parallel |
| `GLOBAL_READ` | Reads state shared by every region | Parallel if the state is immutable or snapshotted |
| `GLOBAL_WRITE` | Writes shared state | Serialized on the owning executor |
| `CROSS_REGION_READ` | Reads data owned by another region | Snapshot, or transaction |
| `CROSS_REGION_WRITE` | Writes data owned by another region | Multi-region transaction |
| `ASYNC_COMPUTE` | Touches no mutable world state | Free to run on any pool |

## Locking rules

* Multi-region operations acquire region locks in **ascending region id order**, always. A lock
  cycle is therefore impossible by construction rather than by care.
* Every acquisition has a timeout. A timeout is a diagnostic event, not a silent retry.
* Lock owners are recorded so the watchdog can name the thread, the region, the chunk range and the
  task that is stuck.
* No global lock around mod code. A single global lock would give a correct server with none of the
  benefit, which is the failure mode this project exists to avoid.

## What the runtime may do when it detects a conflict

In rough order of preference, from cheapest to most disruptive:

1. Serialize the two tasks on the same executor.
2. Defer the losing task to the next tick of its region.
3. Take a snapshot and let the reader work on the copy.
4. Open a multi-region transaction covering both regions.
5. Merge the two regions, if they keep conflicting.
6. Quarantine the offending class or object, lowering its parallelism permanently until it is
   re-tested.

Mods never see a threading exception the runtime can resolve itself.

## Thread identity

* `AutoThreadRuntime.isMainThread()` answers "am I the Minecraft server thread".
* Region context, once regions exist, answers the finer question: "which region am I allowed to
  write right now". Code that cannot answer it is not allowed to write world state at all.

## Rules for new code in this fork

* Patches to Minecraft/Forge/Bukkit classes delegate; they do not contain logic.
* Anything that can block (disk, network, compression, `synchronized` on shared structures) never
  runs on the region tick pool.
* Anything that touches mutable world state never runs on the IO, worker, async or native pools.
* Every new parallel path ships with the metric that proves it is faster and the counter that
  proves it is not conflicting.
