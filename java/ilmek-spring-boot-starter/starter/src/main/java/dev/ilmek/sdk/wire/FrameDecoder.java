package dev.ilmek.sdk.wire;

import static dev.ilmek.sdk.wire.Frame.MAGIC;
import static dev.ilmek.sdk.wire.Frame.MAX_BODY_LENGTH;
import static dev.ilmek.sdk.wire.Frame.MAX_METADATA_LENGTH;
import static dev.ilmek.sdk.wire.Frame.PREFIX_LENGTH;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.Optional;

public final class FrameDecoder {

    private byte[] buffer = new byte[0];
    private int length;

    public void push(byte[] chunk) {
        if (length + chunk.length > buffer.length) {
            buffer = Arrays.copyOf(buffer, Math.max(buffer.length * 2, length + chunk.length));
        }
        System.arraycopy(chunk, 0, buffer, length, chunk.length);
        length += chunk.length;
        validateBufferedHeader();
    }

    public Optional<Frame> nextFrame() {
        validateBufferedHeader();
        if (length < PREFIX_LENGTH) {
            return Optional.empty();
        }
        ByteBuffer header = ByteBuffer.wrap(buffer, 0, PREFIX_LENGTH).order(ByteOrder.BIG_ENDIAN);
        int metadataLength = header.getInt(4);
        int bodyLength = header.getInt(8);
        int total = PREFIX_LENGTH + metadataLength + bodyLength;
        if (length < total) {
            return Optional.empty();
        }
        byte[] metadata = Arrays.copyOfRange(buffer, PREFIX_LENGTH, PREFIX_LENGTH + metadataLength);
        byte[] body = Arrays.copyOfRange(buffer, PREFIX_LENGTH + metadataLength, total);
        System.arraycopy(buffer, total, buffer, 0, length - total);
        length -= total;
        return Optional.of(new Frame(metadata, body));
    }

    private void validateBufferedHeader() {
        int magicSeen = Math.min(length, MAGIC.length);
        for (int i = 0; i < magicSeen; i++) {
            if (buffer[i] != MAGIC[i]) {
                throw new FrameFormatException("bad frame magic");
            }
        }
        if (length < PREFIX_LENGTH) {
            return;
        }
        ByteBuffer header = ByteBuffer.wrap(buffer, 0, PREFIX_LENGTH).order(ByteOrder.BIG_ENDIAN);
        long metadataLength = Integer.toUnsignedLong(header.getInt(4));
        long bodyLength = Integer.toUnsignedLong(header.getInt(8));
        Frame.checkLengths(metadataLength, bodyLength);
    }
}
