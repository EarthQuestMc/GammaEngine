# Building GammaEngine

GammaEngine is a fork of Crucible for Minecraft 1.7.10. It keeps Crucible's patch-based build:
Minecraft and Forge sources are decompiled into a generated workspace, patches from `patches/` are
applied on top, and the fork's own code lives in `src/main/java`.

## Versions this build targets

| Component | Version |
| --- | --- |
| Minecraft | 1.7.10 |
| MCP mappings | 9.08 (stable, `snapshot` channel not used) |
| Forge | 10.13.4.1614 |
| Bukkit API | 1.7.10-R0.1-SNAPSHOT |
| Gradle | 8.0 (wrapper) |
| Build JDK | Java 8 (`JAVA_HOME` must point at a JDK 8) |
| buildSrc toolchain | Java 17, resolved through Gradle toolchains |
| Runtime JDK | Java 8 through 21 (lwjgl3ify is embedded) |

Gradle itself runs on the JDK 8 in `JAVA_HOME`; only the `buildSrc` plugin compiles with a Java 17
toolchain, which Gradle locates automatically (for example `~/.jdks/ms-17.x`). A machine with only
a JDK 8 installed cannot build `buildSrc` from scratch.

## First build

```bash
export JAVA_HOME=/path/to/jdk8          # Windows: set JAVA_HOME to the JDK 8 directory
./gradlew setupCrucible                 # ~2 min: download, deobfuscate, decompile, patch
./gradlew buildPackages                 # ~1 min: compile, reobfuscate, package
```

Artifacts land in `build/distributions/`:

* `Crucible-<mcversion>-<branch>-<hash>-server.jar` — the server,
* `libraries.zip` — every runtime dependency, for machines that cannot download them at boot.

The server jar downloads what it needs on first launch and asks for a restart; that is upstream
Crucible behaviour and is expected.

## What lives where

| Path | Contents |
| --- | --- |
| `src/main/java` | Fork code: CraftBukkit, Crucible, and everything under `io/github/gammaengine` |
| `src/test/java` | Unit tests (JUnit 4), run with `./gradlew :eclipse:cauldron:test` |
| `patches/` | Diffs applied to decompiled Minecraft/Forge sources |
| `eclipse/cauldron/src/main/java` | Generated workspace: decompiled + patched sources. Not in git |
| `buildSrc/` | The `crucible` Gradle plugin that drives setup, patching and reobfuscation |

Editing a Minecraft or Forge class means editing it in `eclipse/cauldron/src/main/java` and then
regenerating the diffs:

```bash
./gradlew genPatches
```

Editing fork-owned code means editing `src/main/java` directly; no patch step is involved. New
GammaEngine subsystems therefore always belong in `src/main/java/io/github/gammaengine`, and the
patch applied to a vanilla class should be a single call into them.

## Useful tasks

| Task | Purpose |
| --- | --- |
| `./gradlew setupCrucible` | Create or repair the workspace |
| `./gradlew :eclipse:cauldron:compileJava` | Fast compile check (~20 s) |
| `./gradlew :eclipse:cauldron:test` | Run the unit tests |
| `./gradlew buildPackages` | Full server jar + libraries archive |
| `./gradlew genPatches` | Regenerate `patches/` from the workspace |
| `./gradlew clean setupCrucible` | Rebuild the workspace after pulling upstream changes |

## Running a development server

```bash
mkdir -p run && cd run
cp ../build/distributions/Crucible-*-server.jar server.jar
echo "eula=true" > eula.txt
java -Xms4G -Xmx8G -jar server.jar nogui     # first run installs libraries and exits
java -Xms4G -Xmx8G -jar server.jar nogui
```

On Java 9 or newer, add the flags from `java9args.txt`.
