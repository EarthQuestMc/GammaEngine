![GammaEngine](logo.png)

# GammaEngine

GammaEngine is a Minecraft **1.7.10** server that runs Forge mods and Bukkit plugins side by side,
and that actually uses several physical CPU cores to simulate the world.

It is a fork of [Crucible](https://github.com/CrucibleMC/Crucible), which is itself a fork of
Thermos. Everything Crucible supports keeps working: the same mods, the same plugins, the same
worlds, the same save format, the same 1.7.10 protocol.

What GammaEngine adds is the **AutoThread runtime**.

## AutoThread in one paragraph

The server decides by itself what can run in parallel. There is no threading mode to choose, no
`legacy` / `hybrid` / `regionized` switch, and nothing for a mod or plugin author to declare. The
runtime watches what code actually touches at runtime, splits the loaded world into regions that do
not interact, ticks independent regions at the same time, and serializes anything it has not proven
safe. When a component misbehaves it loses parallelism at the finest possible granularity, one
class or one object, never a whole mod.

Priority order, and it is not negotiable: world integrity, no silent corruption, compatibility,
stability, then performance.

## Status

Early development. The runtime, its metrics and the native library are in place; region-parallel
simulation is being built on top of them. See [docs/roadmap.md](docs/roadmap.md) for the phase by
phase plan and where the project currently stands.

## Requirements

* Java 8 through 21 (lwjgl3ify is embedded, as in Crucible)
* Forge 1.7.10-10.13.4.1614, Bukkit API 1.7.10-R0.1-SNAPSHOT

## Running

```bash
java -Xms4G -Xmx8G -jar GammaEngine-1.7.10-<version>-server.jar nogui
```

The first launch installs the server libraries and asks for a restart. On Java 9 or newer, add the
flags from `java9args.txt`.

Configuration files, all optional:

| File | Contents |
| --- | --- |
| `Gamma.yml` | General server settings, migrated automatically from `Crucible.yml` |
| `GammaAutoThread.yml` | AutoThread resource limits and diagnostics. No per-mod options, by design |

Commands: `/gamma` (alias `/crucible`) for server information, `/autothread` for runtime status,
worker occupancy, conflicts and profiling.

## Building

See [docs/build.md](docs/build.md). Short version:

```bash
./gradlew setupCrucible     # once, creates the patched workspace
./gradlew buildPackages     # server jar in build/distributions/
cd native && cargo build --release   # optional native acceleration
```

## Documentation

| Document | Contents |
| --- | --- |
| [docs/roadmap.md](docs/roadmap.md) | The phases, what each one does, and current status |
| [docs/architecture.md](docs/architecture.md) | Subsystems and the tick paths they take over |
| [docs/threading-model.md](docs/threading-model.md) | Ownership, access rules, locking, the contract |
| [docs/native-engine.md](docs/native-engine.md) | The Rust library, what it accelerates, measured results |
| [docs/build.md](docs/build.md) | Reproducible build |

## Credits

* [Crucible](https://github.com/CrucibleMC/Crucible) — upstream project
* [Thermos](https://github.com/CyberdyneCC/Thermos) — Crucible's own upstream
* [Spigot](https://hub.spigotmc.org/stash/projects/SPIGOT/repos/spigot/browse) and
  [Paper](https://github.com/PaperMC/Paper) — many improvements over Bukkit
* [lwjgl3ify](https://github.com/GTNewHorizons/lwjgl3ify) — Java 9+ support
