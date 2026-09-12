//! GammaEngine native engine.
//!
//! This library is **optional**. The server boots, runs and saves identical worlds without it; it
//! only makes a handful of batchable, CPU-bound operations faster. Three rules govern everything
//! in here:
//!
//! 1. **No panic crosses the FFI boundary.** Every exported function wraps its body in
//!    [`std::panic::catch_unwind`] and reports failure through a return value the Java side treats
//!    as "use the Java implementation".
//! 2. **No small calls.** A JNI transition costs more than most chunk-sized operations save, so the
//!    exported surface takes whole buffers, never single values in a loop.
//! 3. **No format changes.** Compression and region-file arithmetic reproduce what vanilla writes,
//!    byte for byte.
//!
//! The Java side of the boundary is `io.github.gammaengine.nativeengine.NativeBindings`, and the
//! only class allowed to call it is `NativeEngine`.

pub mod compression;
pub mod hashing;
pub mod region_file;

use jni::objects::{JByteArray, JClass};
use jni::sys::{jbyteArray, jint, jlong};
use jni::JNIEnv;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// ABI version. The Java side refuses to use a library whose version it does not know, which is
/// what keeps a stale `.so` from a previous build out of a running server.
pub const ABI_VERSION: i32 = 1;

/// Runs `body`, converting a panic into `None` so the caller can fall back to Java.
fn guard<T, F: FnOnce() -> Option<T>>(body: F) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(_) => {
            // Printing rather than logging: the Java logger is not reachable from an unwind path,
            // and a silent panic is exactly the failure mode this project refuses.
            eprintln!("[GammaEngine native] panic caught at the FFI boundary, falling back to Java");
            None
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_gammaengine_nativeengine_NativeBindings_abiVersion(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    ABI_VERSION
}

#[no_mangle]
pub extern "system" fn Java_io_github_gammaengine_nativeengine_NativeBindings_zlibCompress<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JByteArray<'local>,
    level: jint,
) -> jbyteArray {
    let result = guard(|| {
        let bytes = env.convert_byte_array(&input).ok()?;
        let compressed = compression::compress(&bytes, level.max(0) as u32).ok()?;
        env.byte_array_from_slice(&compressed).ok()
    });
    match result {
        Some(array) => array.into_raw(),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_gammaengine_nativeengine_NativeBindings_zlibDecompress<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JByteArray<'local>,
    size_hint: jint,
) -> jbyteArray {
    let result = guard(|| {
        let bytes = env.convert_byte_array(&input).ok()?;
        let plain = compression::decompress(&bytes, size_hint.max(0) as usize).ok()?;
        env.byte_array_from_slice(&plain).ok()
    });
    match result {
        Some(array) => array.into_raw(),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_gammaengine_nativeengine_NativeBindings_xxh64<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JByteArray<'local>,
    seed: jlong,
) -> jlong {
    guard(|| {
        let bytes = env.convert_byte_array(&input).ok()?;
        Some(hashing::xxh64(&bytes, seed as u64) as jlong)
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_turns_a_panic_into_a_fallback() {
        let value: Option<i32> = guard(|| panic!("boom"));
        assert!(value.is_none());
    }

    #[test]
    fn guard_passes_results_through() {
        assert_eq!(guard(|| Some(7)), Some(7));
    }
}
