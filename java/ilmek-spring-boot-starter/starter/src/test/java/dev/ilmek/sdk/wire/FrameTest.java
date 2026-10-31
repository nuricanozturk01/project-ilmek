package dev.ilmek.sdk.wire;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import org.junit.jupiter.api.Test;

class FrameTest {

    private static byte[] frameBytes(byte[] magic, int metadataLen, int bodyLen) {
        ByteBuffer buf = ByteBuffer.allocate(12).order(ByteOrder.BIG_ENDIAN);
        buf.put(magic).putInt(metadataLen).putInt(bodyLen);
        return buf.array();
    }

    @Test
    void roundTripsAFrame() {
        byte[] frame = Frame.encode("{\"a\":1}".getBytes(UTF_8), "body".getBytes(UTF_8));
        FrameDecoder d = new FrameDecoder();
        d.push(frame);
        Frame f = d.nextFrame().orElseThrow();
        assertEquals("{\"a\":1}", new String(f.metadata(), UTF_8));
        assertEquals("body", new String(f.body(), UTF_8));
    }

    @Test
    void encodesMagicAndBigEndianLengths() {
        byte[] frame = Frame.encode("{}".getBytes(UTF_8), "xy".getBytes(UTF_8));
        assertArrayEquals(
                new byte[] {'I', 'L', 'M', '1', 0, 0, 0, 2, 0, 0, 0, 2, '{', '}', 'x', 'y'}, frame);
    }

    @Test
    void rejectsBadMagic() {
        byte[] bad = frameBytes("XXXX".getBytes(UTF_8), 2, 0);
        assertThrows(FrameFormatException.class, () -> new FrameDecoder().push(bad));
    }

    @Test
    void rejectsBadMagicWithoutWaitingForFullHeader() {
        FrameDecoder d = new FrameDecoder();
        assertThrows(FrameFormatException.class, () -> d.push(new byte[] {'X'}));
    }

    @Test
    void rejectsZeroLengthMetadata() {
        byte[] bad = frameBytes("ILM1".getBytes(UTF_8), 0, 0);
        assertThrows(FrameFormatException.class, () -> new FrameDecoder().push(bad));
    }

    @Test
    void rejectsMetadataOverMax() {
        byte[] bad = frameBytes("ILM1".getBytes(UTF_8), 65_537, 0);
        assertThrows(FrameFormatException.class, () -> new FrameDecoder().push(bad));
    }

    @Test
    void rejectsBodyOverMax() {
        byte[] bad = frameBytes("ILM1".getBytes(UTF_8), 2, 262_145);
        assertThrows(FrameFormatException.class, () -> new FrameDecoder().push(bad));
    }

    @Test
    void acceptsMaximumLengths() {
        byte[] meta = new byte[65_536];
        byte[] body = new byte[262_144];
        FrameDecoder d = new FrameDecoder();
        d.push(Frame.encode(meta, body));
        Frame f = d.nextFrame().orElseThrow();
        assertEquals(65_536, f.metadata().length);
        assertEquals(262_144, f.body().length);
    }

    @Test
    void buffersPartialReadsAcrossMultiplePushes() {
        byte[] frame = Frame.encode("{}".getBytes(UTF_8), new byte[0]);
        FrameDecoder d = new FrameDecoder();
        d.push(Arrays.copyOfRange(frame, 0, 5));
        assertTrue(d.nextFrame().isEmpty());
        d.push(Arrays.copyOfRange(frame, 5, frame.length));
        assertTrue(d.nextFrame().isPresent());
    }

    @Test
    void retainsExcessBytesForNextFrame() {
        byte[] first = Frame.encode("{\"n\":1}".getBytes(UTF_8), "one".getBytes(UTF_8));
        byte[] second = Frame.encode("{\"n\":2}".getBytes(UTF_8), "two".getBytes(UTF_8));
        byte[] both = new byte[first.length + second.length];
        System.arraycopy(first, 0, both, 0, first.length);
        System.arraycopy(second, 0, both, first.length, second.length);

        FrameDecoder d = new FrameDecoder();
        d.push(both);
        Frame a = d.nextFrame().orElseThrow();
        Frame b = d.nextFrame().orElseThrow();
        assertEquals("one", new String(a.body(), UTF_8));
        assertEquals("two", new String(b.body(), UTF_8));
        assertTrue(d.nextFrame().isEmpty());
    }

    @Test
    void waitsForPayloadBeforeReturningFrame() {
        byte[] frame = Frame.encode("{}".getBytes(UTF_8), "abc".getBytes(UTF_8));
        FrameDecoder d = new FrameDecoder();
        d.push(Arrays.copyOfRange(frame, 0, 13));
        assertTrue(d.nextFrame().isEmpty());
        d.push(Arrays.copyOfRange(frame, 13, frame.length));
        assertEquals("abc", new String(d.nextFrame().orElseThrow().body(), UTF_8));
    }
}
