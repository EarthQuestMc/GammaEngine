package io.github.gammaengine.util;

import org.junit.After;
import org.junit.Test;

import java.util.concurrent.atomic.AtomicBoolean;

import static org.junit.Assert.assertNotSame;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

public class ScratchBuffersTest {

    @After
    public void clearThreadState() {
        ScratchBuffers.release();
    }

    @Test
    public void returnsABufferLargeEnough() {
        assertTrue(ScratchBuffers.get(1000).length >= 1000);
        assertTrue(ScratchBuffers.get(200_000).length >= 200_000);
    }

    @Test
    public void reusesTheSameBufferOnTheSameThread() {
        byte[] first = ScratchBuffers.get(100_000);
        byte[] second = ScratchBuffers.get(100_000);
        assertSame("the point of the class is to allocate once per thread", first, second);
        assertSame("a smaller request must reuse the bigger buffer", first, ScratchBuffers.get(10));
    }

    @Test
    public void neverSharesABufferBetweenThreads() throws Exception {
        // This is the property that makes parallel chunk packet building possible at all.
        final byte[] mine = ScratchBuffers.get(50_000);
        final AtomicBoolean shared = new AtomicBoolean(false);
        Thread other = new Thread(new Runnable() {
            @Override
            public void run() {
                shared.set(ScratchBuffers.get(50_000) == mine);
                ScratchBuffers.release();
            }
        });
        other.start();
        other.join();
        assertTrue("two threads received the same scratch buffer", !shared.get());
    }

    @Test
    public void oversizedRequestsAreNotRetained() {
        byte[] huge = ScratchBuffers.get(8 << 20);
        assertTrue(huge.length >= (8 << 20));
        assertNotSame("a one-off huge buffer must not be pinned to the thread",
                huge, ScratchBuffers.get(8 << 20));
    }

    @Test
    public void releaseDropsTheBuffer() {
        byte[] first = ScratchBuffers.get(10_000);
        ScratchBuffers.release();
        assertNotSame(first, ScratchBuffers.get(10_000));
    }
}
