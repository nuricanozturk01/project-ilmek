package dev.ilmek.sdk.store;

import static java.nio.charset.StandardCharsets.UTF_8;

import java.util.Base64;
import java.util.Optional;

public final class EventCursor {

    public static final int MAX_LENGTH = 512;

    private static final Base64.Encoder ENCODER = Base64.getUrlEncoder().withoutPadding();
    private static final Base64.Decoder DECODER = Base64.getUrlDecoder();

    private EventCursor() {
    }

    public record Parsed(String topic, long sequence) {
    }

    public static String encode(String topic, long sequence) {
        if (topic == null || topic.isEmpty()) {
            throw new IllegalArgumentException("topic is required");
        }
        if (sequence < 0) {
            throw new IllegalArgumentException("sequence must not be negative");
        }
        String cursor = ENCODER.encodeToString((topic + ":" + sequence).getBytes(UTF_8));
        if (cursor.length() > MAX_LENGTH) {
            throw new IllegalArgumentException("topic too long for cursor");
        }
        return cursor;
    }

    public static Optional<Parsed> decode(String cursor) {
        if (cursor == null || cursor.isEmpty() || cursor.length() > MAX_LENGTH) {
            return Optional.empty();
        }
        String text;
        try {
            text = new String(DECODER.decode(cursor), UTF_8);
        } catch (IllegalArgumentException e) {
            return Optional.empty();
        }
        int colon = text.lastIndexOf(':');
        if (colon <= 0) {
            return Optional.empty();
        }
        long sequence;
        try {
            sequence = Long.parseLong(text.substring(colon + 1));
        } catch (NumberFormatException e) {
            return Optional.empty();
        }
        if (sequence < 0) {
            return Optional.empty();
        }
        Parsed parsed = new Parsed(text.substring(0, colon), sequence);
        return cursor.equals(encode(parsed.topic(), parsed.sequence())) ? Optional.of(parsed) : Optional.empty();
    }
}
