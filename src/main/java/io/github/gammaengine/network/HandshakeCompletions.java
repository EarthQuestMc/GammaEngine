package io.github.gammaengine.network;

import java.lang.ref.WeakReference;
import java.util.Collections;
import java.util.Map;
import java.util.WeakHashMap;

import cpw.mods.fml.common.network.FMLIndexedMessageToMessageCodec;
import cpw.mods.fml.common.network.handshake.NetworkDispatcher;
import cpw.mods.fml.common.network.internal.FMLProxyPacket;
import io.netty.channel.ChannelHandlerContext;
import io.netty.util.Attribute;

/**
 * Pairs each FML "complete handshake" packet with the connection it completes.
 *
 * <p>FML keeps a single {@code FML} EmbeddedChannel for every connection. Forge 1.7.10 passed the
 * connection from {@code FMLNetworkHandler.forwardHandshake} (network thread) to
 * {@code HandshakeCompletionHandler} (main thread, one tick later) through an attribute of that
 * shared channel: two handshakes finishing within the same tick overwrote each other, and one
 * player never joined. The pairing now lives here, keyed by the packet itself.
 *
 * <p>Only packets registered by {@code forwardHandshake} complete a connection: a client that
 * sends its own "complete handshake" on the {@code FML} channel cannot skip the mod list check.
 * Keys are weak, so a packet dropped with its connection before the main thread reads it does
 * not keep the connection alive.
 */
public final class HandshakeCompletions {
    private static final Map<FMLProxyPacket, NetworkDispatcher> PENDING =
            Collections.synchronizedMap(new WeakHashMap<FMLProxyPacket, NetworkDispatcher>());

    private HandshakeCompletions() {
    }

    /** Records that {@code packet}, built by forwardHandshake, completes the connection of {@code dispatcher}. */
    public static void register(FMLProxyPacket packet, NetworkDispatcher dispatcher) {
        PENDING.put(packet, dispatcher);
    }

    /** Removes and returns the connection {@code packet} completes, or null when forwardHandshake did not build it. */
    public static NetworkDispatcher take(FMLProxyPacket packet) {
        return packet == null ? null : PENDING.remove(packet);
    }

    /**
     * Removes and returns the connection completed by the packet that the FML codec of this pipeline
     * is decoding on the current thread, or null when there is none.
     */
    public static NetworkDispatcher take(ChannelHandlerContext ctx) {
        // Netty 4.0 keeps attributes per handler context: the tracker lives on the codec's context.
        ChannelHandlerContext codec = ctx.pipeline().context(FMLIndexedMessageToMessageCodec.class);
        if (codec == null) {
            return null;
        }
        Attribute<ThreadLocal<WeakReference<FMLProxyPacket>>> tracker = codec.attr(FMLIndexedMessageToMessageCodec.INBOUNDPACKETTRACKER);
        ThreadLocal<WeakReference<FMLProxyPacket>> perThread = tracker.get();
        WeakReference<FMLProxyPacket> reference = perThread == null ? null : perThread.get();
        return take(reference == null ? null : reference.get());
    }

    /** Number of packets waiting for the main thread; for tests. */
    static int pending() {
        return PENDING.size();
    }
}
