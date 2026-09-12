package io.github.gammaengine.util;

/**
 * Per-thread scratch byte arrays for transient work such as compression output.
 *
 * <p>The pattern this replaces is everywhere in the packet code: allocate a buffer as large as the
 * uncompressed data, compress into it, then keep that buffer as the packet payload. A chunk packet
 * ends up holding roughly 164 kB alive to carry about 20 kB of compressed bytes, for as long as the
 * packet sits in a slow client's queue. Multiply by a few hundred players and it is gigabytes of
 * heap that exist only because nobody trimmed a buffer.
 *
 * <p>A scratch buffer is borrowed, written into, and the caller copies out exactly the bytes it
 * needs. The buffer itself stays with the thread and is reused, so the large allocation happens
 * once per thread instead of once per packet.
 *
 * <p>Rules for callers:
 * <ul>
 *   <li>never keep a reference to the returned array,</li>
 *   <li>never pass it to another thread,</li>
 *   <li>assume its contents are garbage on every call.</li>
 * </ul>
 */
public final class ScratchBuffers {
    /**
     * Buffers above this size are not retained between calls. A one-off huge chunk should not make
     * a Netty thread hold megabytes forever.
     */
    private static final int MAX_RETAINED = 1 << 21; // 2 MiB

    private static final ThreadLocal<byte[]> SCRATCH = new ThreadLocal<byte[]>();

    private ScratchBuffers() {
    }

    /**
     * Returns a scratch array of at least {@code minimumSize} bytes for the calling thread.
     *
     * <p>The array is often larger than requested, so callers must track how many bytes they wrote
     * and copy out that many.
     */
    public static byte[] get(int minimumSize) {
        if (minimumSize > MAX_RETAINED) {
            // Too big to keep around: hand out a throwaway array rather than pinning it to a thread.
            return new byte[minimumSize];
        }
        byte[] existing = SCRATCH.get();
        if (existing != null && existing.length >= minimumSize) {
            return existing;
        }
        // Grow with headroom so a slowly increasing demand does not reallocate on every call.
        int size = Math.max(minimumSize, existing == null ? 0 : existing.length * 2);
        byte[] created = new byte[Math.min(MAX_RETAINED, size)];
        if (created.length < minimumSize) {
            return new byte[minimumSize];
        }
        SCRATCH.set(created);
        return created;
    }

    /** Drops the calling thread's scratch buffer. Used when a thread is about to go idle for long. */
    public static void release() {
        SCRATCH.remove();
    }
}
