package io.github.gammaengine.network;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.BeforeClass;
import org.junit.Test;

import cpw.mods.fml.common.DummyModContainer;
import cpw.mods.fml.common.ModMetadata;
import cpw.mods.fml.common.network.FMLEmbeddedChannel;
import cpw.mods.fml.common.network.FMLOutboundHandler;
import cpw.mods.fml.common.network.FMLOutboundHandler.OutboundTarget;
import cpw.mods.fml.common.network.NetworkRegistry;
import cpw.mods.fml.common.network.handshake.NetworkDispatcher;
import cpw.mods.fml.common.network.internal.FMLMessage.CompleteHandshake;
import cpw.mods.fml.common.network.internal.FMLNetworkHandler;
import cpw.mods.fml.common.network.internal.FMLProxyPacket;
import cpw.mods.fml.common.network.internal.FMLRuntimeCodec;
import cpw.mods.fml.common.network.internal.HandshakeCompletionHandler;
import cpw.mods.fml.relauncher.Side;
import net.minecraft.network.NetworkManager;

/**
 * The FML handshake completion, through the real FML channel: forwardHandshake on a network
 * thread, then FMLProxyPacket.processPacket on the main thread, as NetworkDispatcher and
 * NetworkManager chain them.
 */
public class HandshakeCompletionsTest {
    private static FMLEmbeddedChannel serverChannel;

    /** A server-side dispatcher that only counts its completions. */
    static final class RecordingDispatcher extends NetworkDispatcher {
        final AtomicInteger completions = new AtomicInteger();

        RecordingDispatcher() {
            super(new NetworkManager(false), null);
        }

        @Override
        public void completeHandshake(Side target) {
            completions.incrementAndGet();
        }
    }

    /** Builds the FML channel pair as FMLNetworkHandler.registerChannel does, without a running Loader. */
    @BeforeClass
    public static void registerFmlChannel() throws Exception {
        ModMetadata metadata = new ModMetadata();
        metadata.modId = "FML";
        EnumMap<Side, FMLEmbeddedChannel> pair = NetworkRegistry.INSTANCE.newChannel(new DummyModContainer(metadata), "FML",
                new FMLRuntimeCodec(), new HandshakeCompletionHandler());
        pair.get(Side.SERVER).attr(FMLOutboundHandler.FML_MESSAGETARGET).set(OutboundTarget.NOWHERE);
        Field channelPair = FMLNetworkHandler.class.getDeclaredField("channelPair");
        channelPair.setAccessible(true);
        channelPair.set(null, pair);
        serverChannel = pair.get(Side.SERVER);
    }

    /** What NetworkDispatcher does on the network thread once the FML|HS handshake is done. */
    private static List<FMLProxyPacket> forward(NetworkDispatcher dispatcher) {
        List<FMLProxyPacket> packets = FMLNetworkHandler.forwardHandshake(new CompleteHandshake(Side.SERVER), dispatcher, Side.SERVER);
        for (FMLProxyPacket packet : packets) {
            packet.setTarget(Side.SERVER);
            packet.payload().resetReaderIndex();
        }
        return packets;
    }

    /** What the main thread does with each packet NetworkManager queued. */
    private static void process(List<FMLProxyPacket> packets) {
        for (FMLProxyPacket packet : packets) {
            packet.processPacket(null);
        }
    }

    @Test
    public void twoHandshakesFinishingInTheSameTickBothComplete() {
        RecordingDispatcher first = new RecordingDispatcher();
        RecordingDispatcher second = new RecordingDispatcher();

        // Both network threads finish before the main thread reads either packet.
        List<FMLProxyPacket> firstPackets = forward(first);
        List<FMLProxyPacket> secondPackets = forward(second);
        assertEquals(1, firstPackets.size());
        assertEquals(1, secondPackets.size());

        process(firstPackets);
        assertEquals("the first packet completes the first connection", 1, first.completions.get());
        assertEquals(0, second.completions.get());
        process(secondPackets);
        assertEquals(1, first.completions.get());
        assertEquals("the second connection is not lost", 1, second.completions.get());
    }

    @Test
    public void forwardHandshakeLeavesNothingOnTheSharedChannel() {
        forward(new RecordingDispatcher());
        // While the dispatcher stayed in the attribute, main thread writes were queued here instead of sent.
        assertNull(serverChannel.attr(NetworkDispatcher.FML_DISPATCHER).get());
        assertTrue(serverChannel.outboundMessages().isEmpty());
    }

    @Test
    public void aCompletionTheServerDidNotBuildCompletesNothing() {
        RecordingDispatcher honest = new RecordingDispatcher();
        RecordingDispatcher forger = new RecordingDispatcher();
        List<FMLProxyPacket> honestPackets = forward(honest);

        // A client may send the same bytes on the FML channel; NetworkDispatcher proxies it with its own dispatcher.
        FMLProxyPacket forged = (FMLProxyPacket) serverChannel.generatePacketFrom(new CompleteHandshake(Side.SERVER));
        forged.setTarget(Side.SERVER);
        forged.setDispatcher(forger);
        forged.processPacket(null);
        assertEquals("a forged completion does not skip the mod list check", 0, forger.completions.get());
        assertEquals("nor completes the connection waiting for the main thread", 0, honest.completions.get());

        process(honestPackets);
        assertEquals(1, honest.completions.get());
    }

    @Test
    public void eachPacketCompletesOnce() {
        RecordingDispatcher dispatcher = new RecordingDispatcher();
        List<FMLProxyPacket> packets = forward(dispatcher);
        process(packets);
        for (FMLProxyPacket packet : packets) {
            packet.payload().resetReaderIndex();
        }
        process(packets);
        assertEquals(1, dispatcher.completions.get());
    }

    @Test(timeout = 60000)
    public void concurrentHandshakesEachCompleteTheirOwnConnection() throws Exception {
        final int threads = 4;
        final int perThread = 100;
        final List<RecordingDispatcher> dispatchers = new ArrayList<RecordingDispatcher>();
        for (int i = 0; i < threads * perThread; i++) {
            dispatchers.add(new RecordingDispatcher());
        }
        @SuppressWarnings("unchecked")
        final List<FMLProxyPacket>[] results = new List[dispatchers.size()];
        final AtomicReference<Throwable> failure = new AtomicReference<Throwable>();
        final CountDownLatch start = new CountDownLatch(1);
        List<Thread> workers = new ArrayList<Thread>();
        for (int t = 0; t < threads; t++) {
            final int offset = t * perThread;
            Thread worker = new Thread(new Runnable() {
                @Override
                public void run() {
                    try {
                        start.await();
                        for (int i = offset; i < offset + perThread; i++) {
                            results[i] = forward(dispatchers.get(i));
                        }
                    } catch (Throwable e) {
                        failure.compareAndSet(null, e);
                    }
                }
            }, "Netty Server IO #" + t);
            workers.add(worker);
            worker.start();
        }
        start.countDown();
        for (Thread worker : workers) {
            worker.join();
        }
        if (failure.get() != null) {
            throw new AssertionError(failure.get());
        }

        for (int i = 0; i < dispatchers.size(); i++) {
            assertEquals("packets of connection " + i, 1, results[i].size());
        }
        // The main thread reads them in another order than they were built.
        for (int i = dispatchers.size() - 1; i >= 0; i--) {
            process(results[i]);
        }
        for (int i = 0; i < dispatchers.size(); i++) {
            assertEquals("completions of connection " + i, 1, dispatchers.get(i).completions.get());
        }
    }

    @Test
    public void takeOfAnUnknownOrNullPacketIsNull() {
        assertNull(HandshakeCompletions.take((FMLProxyPacket) null));
        FMLProxyPacket unknown = (FMLProxyPacket) serverChannel.generatePacketFrom(new CompleteHandshake(Side.SERVER));
        assertNull(HandshakeCompletions.take(unknown));
    }
}
