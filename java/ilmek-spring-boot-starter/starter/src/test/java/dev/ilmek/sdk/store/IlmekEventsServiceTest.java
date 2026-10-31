package dev.ilmek.sdk.store;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Propagation;
import org.springframework.transaction.annotation.Transactional;

@DataJpaTest
@Import(IlmekEventsService.class)
@Transactional(propagation = Propagation.NOT_SUPPORTED)
class IlmekEventsServiceTest {

    @Autowired
    private IlmekEventsService events;

    @Autowired
    private IlmekEventRepository repository;

    @AfterEach
    void clean() {
        repository.deleteAll();
    }

    @Test
    void appendAssignsIncrementingSequencePerTopic() {
        var e1 = events.append("tasks.events", "a".getBytes(UTF_8));
        var e2 = events.append("tasks.events", "b".getBytes(UTF_8));
        assertEquals(e1.getSequence() + 1, e2.getSequence());
    }

    @Test
    void appendStartsAtOneForEmptyTopicAndKeepsTopicsIndependent() {
        var first = events.append("alpha", "a".getBytes(UTF_8));
        events.append("beta", "b".getBytes(UTF_8));
        var secondAlpha = events.append("alpha", "c".getBytes(UTF_8));

        assertEquals(1, first.getSequence());
        assertEquals(2, secondAlpha.getSequence());
        assertEquals(1, events.append("gamma", "d".getBytes(UTF_8)).getSequence());
    }

    @Test
    void appendStoresTopicEventIdBodyAndTimestamp() {
        var e = events.append("t", "payload".getBytes(UTF_8));

        assertEquals("t", e.getTopic());
        assertEquals(36, e.getEventId().length());
        assertEquals(7, e.getEventId().charAt(14) - '0');
        assertEquals("payload", new String(e.getBody(), UTF_8));
        assertTrue(e.getCreatedAtMs() > 0);
    }

    @Test
    void sinceNullCursorReturnsAllFromStart() {
        events.append("t", "a".getBytes(UTF_8));
        events.append("t", "b".getBytes(UTF_8));
        events.append("t", "c".getBytes(UTF_8));

        List<IlmekEvent> all = events.since("t", null);

        assertEquals(3, all.size());
        assertEquals("a", new String(all.get(0).getBody(), UTF_8));
        assertEquals("b", new String(all.get(1).getBody(), UTF_8));
        assertEquals("c", new String(all.get(2).getBody(), UTF_8));
        assertTrue(all.get(0).getSequence() < all.get(1).getSequence());
        assertTrue(all.get(1).getSequence() < all.get(2).getSequence());
    }

    @Test
    void sinceWithCursorReturnsOnlyLaterEvents() {
        var e1 = events.append("t", "a".getBytes(UTF_8));
        events.append("t", "b".getBytes(UTF_8));
        String cursor = EventCursor.encode("t", e1.getSequence());

        assertEquals(1, events.since("t", cursor).size());
    }

    @Test
    void sinceDoesNotReturnEventsFromOtherTopics() {
        events.append("t", "a".getBytes(UTF_8));
        events.append("other", "x".getBytes(UTF_8));

        List<IlmekEvent> result = events.since("t", null);

        assertEquals(1, result.size());
        assertEquals("t", result.get(0).getTopic());
    }

    @Test
    void sinceRejectsCursorForAnotherTopic() {
        var e1 = events.append("t", "a".getBytes(UTF_8));
        events.append("other", "x".getBytes(UTF_8));
        String foreignCursor = EventCursor.encode("other", e1.getSequence());

        assertThrows(IllegalArgumentException.class, () -> events.since("t", foreignCursor));
    }

    @Test
    void sinceRejectsGarbageCursor() {
        events.append("t", "a".getBytes(UTF_8));

        assertThrows(IllegalArgumentException.class, () -> events.since("t", "not-a-cursor!"));
    }

    @Test
    void decodeRejectsGarbageCursor() {
        assertTrue(EventCursor.decode("not-a-cursor").isEmpty());
    }

    @Test
    void cursorRoundTripsTopicContainingColons() {
        String cursor = EventCursor.encode("tasks:v2:events", 42);

        var parsed = EventCursor.decode(cursor).orElseThrow();
        assertEquals("tasks:v2:events", parsed.topic());
        assertEquals(42, parsed.sequence());
        assertTrue(cursor.length() <= 512);
    }

    @Test
    void decodeRejectsNullAndOversizedCursor() {
        assertTrue(EventCursor.decode(null).isEmpty());
        assertTrue(EventCursor.decode("A".repeat(513)).isEmpty());
    }

    @Test
    void retentionTrimsBeyond10000PerTopic() {
        for (int i = 0; i < 10005; i++) {
            events.append("t", ("e" + i).getBytes(UTF_8));
        }

        assertEquals(10000, events.countForTopic("t"));
        List<IlmekEvent> retained = events.since("t", null);
        assertEquals(6, retained.get(0).getSequence());
        assertEquals(10005, retained.get(retained.size() - 1).getSequence());
    }

    @Test
    void retentionDoesNotTrimOtherTopics() {
        events.append("quiet", "q".getBytes(UTF_8));
        for (int i = 0; i < 10001; i++) {
            events.append("busy", "b".getBytes(UTF_8));
        }

        assertEquals(1, events.countForTopic("quiet"));
        assertEquals(10000, events.countForTopic("busy"));
    }
}
