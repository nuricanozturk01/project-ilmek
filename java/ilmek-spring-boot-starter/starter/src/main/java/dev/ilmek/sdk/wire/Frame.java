package dev.ilmek.sdk.wire;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

public record Frame(byte[] metadata, byte[] body) {

    static final byte[] MAGIC = {'I', 'L', 'M', '1'};
    static final int PREFIX_LENGTH = 12;
    static final int MAX_METADATA_LENGTH = 65_536;
    static final int MAX_BODY_LENGTH = 262_144;

    public static byte[] encode(byte[] metadataJson, byte[] body) {
        Frame.checkLengths(metadataJson.length, body.length);
        return ByteBuffer.allocate(PREFIX_LENGTH + metadataJson.length + body.length)
                .order(ByteOrder.BIG_ENDIAN)
                .put(MAGIC)
                .putInt(metadataJson.length)
                .putInt(body.length)
                .put(metadataJson)
                .put(body)
                .array();
    }

    static void checkLengths(long metadataLength, long bodyLength) {
        if (metadataLength < 1 || metadataLength > MAX_METADATA_LENGTH) {
            throw new FrameFormatException("metadata length outside 1.." + MAX_METADATA_LENGTH + ": " + metadataLength);
        }
        if (bodyLength < 0 || bodyLength > MAX_BODY_LENGTH) {
            throw new FrameFormatException("body length outside 0.." + MAX_BODY_LENGTH + ": " + bodyLength);
        }
    }
}
