# Native engine (Rust)

The native engine is an **optional** Rust library that accelerates a few batchable, CPU-bound
operations. The server boots, runs and produces identical worlds without it. When
`libgammaengine_native.so` (or `gammaengine_native.dll`) is missing, fails to load, or reports a
different ABI version, the engine logs one line and uses the Java implementations.

## What it accelerates, and why those things

Rust is used where three conditions hold at once: the work is CPU-bound, it arrives in buffers big
enough to amortise a JNI transition, and it touches no mutable world state. That rules out the tick
loop, entities, tile entities, the Forge event bus and Bukkit, and it rules in:

| Operation | Where it is used | Status |
| --- | --- | --- |
| zlib compress | Writing a chunk into a region file | Implemented |
| zlib decompress | Reading a chunk from a region file | Implemented |
| XXH64 | Chunk snapshot hashing, world integrity and determinism checks | Implemented |
| Region-file sector arithmetic and validation | Region file repair and allocation | Implemented (pure functions) |
| NBT binary encode/decode | Chunk save/load pipeline | Planned |
| Pathfinding on immutable snapshots | Mob AI, off the region thread | Planned, phase 10 |
| Batch collision and spatial queries | Entity movement | Planned, phase 10 |

## Measured results

Reference payload: 200 000 bytes of chunk-shaped data (long runs of identical blocks broken by
noisy regions). Median of 50 runs after 200 warm-up iterations, JNI transition and array copies
included. Machine: development workstation, not the EPYC target; numbers are for the ratio, not the
absolute value.

| Operation | Java | Rust | Change |
| --- | --- | --- | --- |
| zlib compress (level 6) | 195 MiB/s | 265 MiB/s | +36% |
| zlib decompress | 1010 MiB/s | 1544 MiB/s | +53% |
| XXH64 | 4296 MiB/s | 7509 MiB/s | +75% |
| Compressed size | 19 268 bytes | 18 806 bytes | −2.4% |

Reproduce with:

```bash
cd native && cargo build --release && cd ..
./gradlew :eclipse:cauldron:test --tests '*NativeEngineTest*'
# results in eclipse/cauldron/build/test-results/test/TEST-*NativeEngineTest.xml
```

A future acceleration that does not beat Java on this harness does not get merged.

## Safety rules

* **No panic crosses the boundary.** Every exported function wraps its body in `catch_unwind` and
  returns a null array (or 0) on failure. The Java side treats that as "use Java", counts it, and
  continues.
* **No small calls.** Buffers below 4 KiB stay in Java: the transition and the two array copies cost
  more than they save. The threshold lives in `NativeEngine.MIN_NATIVE_BYTES`.
* **One façade.** `NativeBindings` holds every `native` declaration and is package-private;
  `NativeEngine` is the only caller and owns the fallback, the metrics and the ABI check.
* **ABI versioning.** `ABI_VERSION` exists in both `native/src/lib.rs` and `NativeEngine`. A
  mismatch disables native acceleration with a warning instead of failing mysteriously later.
* **Same results, not same bytes.** Java's `Deflater` and the Rust encoder emit different but
  equally valid zlib streams. Region files stay readable by vanilla, Forge and external tools
  because both are valid zlib; what the tests pin down is that each side reads the other's output
  and that the decompressed content and its hash are identical. World hashes are therefore always
  computed on decompressed content, never on compressed bytes.

## Building

```bash
cd native
cargo test            # 17 unit tests, including a cross-check against an independent XXH64
cargo build --release # target/release/libgammaengine_native.so (or .dll)
```

Install it where the server can find it, in order of precedence:

1. `-Dgammaengine.nativeLibrary=/absolute/path/to/libgammaengine_native.so`
2. anywhere on `java.library.path`
3. `gammaengine/native/libgammaengine_native.so` next to the server jar

## Metrics

`/autothread native` reports whether the library is loaded and where from. The profiler records
`native.compress`, `native.decompress`, `native.hash`, the Java counterparts under
`native.*.java`, and the counters `native.jni.calls`, `native.jni.bytes` and
`native.fallback.*`. A rising fallback counter means the native path is refusing work and should be
investigated rather than ignored.
