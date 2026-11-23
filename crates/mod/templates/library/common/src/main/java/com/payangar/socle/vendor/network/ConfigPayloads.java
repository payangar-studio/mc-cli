package com.payangar.socle.vendor.network;

import io.netty.buffer.ByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/**
 * The network payloads of one mod, named {@code <modid>:vendor/<name>}: each copy of the library
 * has its own, so several mods carrying it run together. The settings travel as JSON, written and
 * read by the setting types of the schema.
 */
public final class ConfigPayloads {
    /**
     * The longest JSON of the server settings, in characters: three bytes each at most, within the
     * 1 MiB a payload sent to a client may weigh.
     */
    static final int MAX_SYNC = 262_144;

    /** The server settings, sent to a player when they join and after every change. */
    public record Sync(Type<Sync> type, String values) implements CustomPacketPayload {}

    /**
     * Never sent: a client registers it to show it has the mod, which the server checks while it
     * configures the connection.
     */
    public record Presence(Type<Presence> type) implements CustomPacketPayload {}

    private final CustomPacketPayload.Type<Sync> syncType;
    private final CustomPacketPayload.Type<Presence> presenceType;
    private final StreamCodec<ByteBuf, Sync> syncCodec;
    private final StreamCodec<ByteBuf, Presence> presenceCodec;

    ConfigPayloads(String modId) {
        syncType = new CustomPacketPayload.Type<>(Identifier.fromNamespaceAndPath(modId, "vendor/config_sync"));
        presenceType = new CustomPacketPayload.Type<>(Identifier.fromNamespaceAndPath(modId, "vendor/presence"));
        syncCodec = ByteBufCodecs.stringUtf8(MAX_SYNC).map(this::sync, Sync::values);
        presenceCodec = StreamCodec.unit(new Presence(presenceType));
    }

    public Sync sync(String values) {
        return new Sync(syncType, values);
    }

    public CustomPacketPayload.Type<Sync> syncType() {
        return syncType;
    }

    public CustomPacketPayload.Type<Presence> presenceType() {
        return presenceType;
    }

    public StreamCodec<ByteBuf, Sync> syncCodec() {
        return syncCodec;
    }

    public StreamCodec<ByteBuf, Presence> presenceCodec() {
        return presenceCodec;
    }
}
