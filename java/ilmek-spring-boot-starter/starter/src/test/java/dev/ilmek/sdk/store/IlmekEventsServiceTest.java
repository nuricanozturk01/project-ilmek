package dev.ilmek.sdk.store;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
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

    @Autowired
    private IlmekEventFloorRepository floors;

    @AfterEach
    void clean() {
        repository.deleteAll();
        floors.deleteAll();
    }

    private static byte[] obj(String v) {
        return json(v).getBytes(UTF_8);
    }

    private static String json(String v) {
        return "{\"v\":\"" + v + "\"}";
    }

    @Test
    void appendAssignsIncrementingSequencePerTopic() {
        var e1 = events.append("tasks.events", obj("a"));
        var e2 = events.append("tasks.events", obj("b"));
        assertEquals(e1.getSequence() + 1, e2.getSequence());
    }

    @Test
    void appendStartsAtOneForEmptyTopicAndKeepsTopicsIndependent() {
        var first = events.append("alpha", obj("a"));
        events.append("beta", obj("b"));
        var secondAlpha = events.append("alpha", obj("c"));

        assertEquals(1, first.getSequence());
        assertEquals(2, secondAlpha.getSequence());
        assertEquals(1, events.append("gamma", obj("d")).getSequence());
    }

    @Test
    void appendStoresTopicEventIdBodyAndTimestamp() {
        var e = events.append("t", obj("payload"));

        assertEquals("t", e.getTopic());
        assertEquals(36, e.getEventId().length());
        assertEquals(7, e.getEventId().charAt(14) - '0');
        assertEquals(json("payload"), new String(e.getBody(), UTF_8));
        assertTrue(e.getCreatedAtMs() > 0);
    }

    @Test
    void sinceNullCursorReturnsAllFromStart() {
        events.append("t", obj("a"));
        events.append("t", obj("b"));
        events.append("t", obj("c"));

        List<IlmekEvent> all = events.since("t", null);

        assertEquals(3, all.size());
        assertEquals(json("a"), new String(all.get(0).getBody(), UTF_8));
        assertEquals(json("b"), new String(all.get(1).getBody(), UTF_8));
        assertEquals(json("c"), new String(all.get(2).getBody(), UTF_8));
        assertTrue(all.get(0).getSequence() < all.get(1).getSequence());
        assertTrue(all.get(1).getSequence() < all.get(2).getSequence());
    }

    @Test
    void sinceWithCursorReturnsOnlyLaterEvents() {
        var e1 = events.append("t", obj("a"));
        events.append("t", obj("b"));
        String cursor = EventCursor.encode("t", e1.getSequence());

        List<IlmekEvent> later = events.since("t", cursor);

        assertEquals(1, later.size());
        assertEquals(json("b"), new String(later.get(0).getBody(), UTF_8));
        assertTrue(later.get(0).getSequence() > e1.getSequence());
    }

    @Test
    void sinceDoesNotReturnEventsFromOtherTopics() {
        events.append("t", obj("a"));
        events.append("other", obj("x"));

        List<IlmekEvent> result = events.since("t", null);

        assertEquals(1, result.size());
        assertEquals("t", result.get(0).getTopic());
    }

    @Test
    void sinceRejectsCursorForAnotherTopic() {
        var e1 = events.append("t", obj("a"));
        events.append("other", obj("x"));
        String foreignCursor = EventCursor.encode("other", e1.getSequence());

        assertThrows(IllegalArgumentException.class, () -> events.since("t", foreignCursor));
    }

    @Test
    void sinceRejectsGarbageCursor() {
        events.append("t", obj("a"));

        assertThrows(IllegalArgumentException.class, () -> events.since("t", "not-a-cursor!"));
    }

    @Test
    void appendRejectsBodyLargerThanLimitAndPersistsNothing() {
        String oversized = "{\"v\":\"" + "x".repeat(EventBody.MAX_BYTES) + "\"}";

        assertThrows(IllegalArgumentException.class,
            () -> events.append("big", oversized.getBytes(UTF_8)));

        assertEquals(0, events.countForTopic("big"));
        assertEquals(0, events.latestSequence("big"));
    }

    @Test
    void appendRejectsJsonArrayBodyAndPersistsNothing() {
        assertThrows(IllegalArgumentException.class,
            () -> events.append("arr", "[1,2]".getBytes(UTF_8)));
        assertThrows(IllegalArgumentException.class,
            () -> events.append("arr", "not json".getBytes(UTF_8)));

        assertEquals(0, events.countForTopic("arr"));
    }

    @Test
    void appendAcceptsJsonObjectBody() {
        var saved = events.append("obj", "{\"n\":1}".getBytes(UTF_8));

        assertEquals(1, saved.getSequence());
        assertEquals("{\"n\":1}", new String(saved.getBody(), UTF_8));
    }

    @Test
    void pageReturnsAtMostOneWindowOfEventsAfterCursor() {
        for (int i = 0; i < 40; i++) {
            events.append("paged", obj("e" + i));
        }

        List<IlmekEvent> first = events.page("paged", 0);

        assertEquals(IlmekEventsService.PAGE_SIZE, first.size());
        assertTrue(first.size() <= 16);
        assertEquals(1, first.get(0).getSequence());
        assertEquals(16, first.get(first.size() - 1).getSequence());
        assertEquals(17, events.page("paged", 16).get(0).getSequence());
    }

    @Test
    void validateCursorReturnsSequenceWithoutLoadingEvents() {
        var e1 = events.append("t", obj("a"));
        String cursor = EventCursor.encode("t", e1.getSequence());

        assertEquals(e1.getSequence(), events.validateCursor("t", cursor));
        assertThrows(IllegalArgumentException.class,
            () -> events.validateCursor("t", EventCursor.encode("other", 1)));
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
            events.append("t", obj("e" + i));
        }

        assertEquals(10000, events.countForTopic("t"));
        List<IlmekEvent> retained = events.since("t", null);
        assertEquals(6, retained.get(0).getSequence());
        assertEquals(10005, retained.get(retained.size() - 1).getSequence());
    }

    @Test
    void retentionDoesNotTrimOtherTopics() {
        events.append("quiet", obj("q"));
        for (int i = 0; i < 10001; i++) {
            events.append("busy", obj("b"));
        }

        assertEquals(1, events.countForTopic("quiet"));
        assertEquals(10000, events.countForTopic("busy"));
    }

    @Test
    void neverTrimmedTopicNeverExpires() {
        events.append("t", obj("a"));
        events.append("t", obj("b"));

        List<IlmekEvent> result = assertDoesNotThrow(() -> events.since("t", EventCursor.encode("t", 0)));

        assertEquals(2, result.size());
        assertFalse(floors.existsById("t"));
    }

    @Test
    void cursorBelowTrimFloorIsExpired() {
        for (int i = 0; i < 10005; i++) {
            events.append("t", obj("e" + i));
        }

        assertThrows(CursorExpiredException.class, () -> events.since("t", EventCursor.encode("t", 4)));
        assertThrows(CursorExpiredException.class, () -> events.since("t", EventCursor.encode("t", 0)));
    }

    @Test
    void cursorExactlyAtTrimFloorIsValidAndReturnsRetainedEvents() {
        for (int i = 0; i < 10005; i++) {
            events.append("t", obj("e" + i));
        }

        List<IlmekEvent> result = events.since("t", EventCursor.encode("t", 5));

        assertEquals(10000, result.size());
        assertEquals(6, result.get(0).getSequence());
        assertEquals(json("e5"), new String(result.get(0).getBody(), UTF_8));
    }

    @Test
    void trimFloorIncreasesAcrossTrimPasses() {
        for (int i = 0; i < 10005; i++) {
            events.append("t", obj("e" + i));
        }
        for (int i = 0; i < 1000; i++) {
            events.append("t", obj("f" + i));
        }

        assertEquals(1005, floors.findById("t").orElseThrow().getFloorSequence());
        assertThrows(CursorExpiredException.class, () -> events.since("t", EventCursor.encode("t", 5)));
        assertThrows(CursorExpiredException.class, () -> events.since("t", EventCursor.encode("t", 1004)));
        List<IlmekEvent> result = events.since("t", EventCursor.encode("t", 1005));
        assertEquals(10000, result.size());
        assertEquals(1006, result.get(0).getSequence());
    }

    @Test
    void trimFloorIsOnlyRecordedForTrimmedTopic() {
        events.append("quiet", obj("q"));
        for (int i = 0; i < 10001; i++) {
            events.append("busy", obj("b"));
        }

        assertFalse(floors.existsById("quiet"));
        assertEquals(1, floors.findById("busy").orElseThrow().getFloorSequence());
        assertEquals(1, events.since("quiet", EventCursor.encode("quiet", 0)).size());
    }

    @Test
    void latestSequenceIsZeroForEmptyTopicAndTracksAppendsPerTopic() {
        assertEquals(0, events.latestSequence("empty.topic"));
        events.append("latest.a", obj("a"));
        events.append("latest.a", obj("b"));
        events.append("latest.b", obj("c"));

        assertEquals(2, events.latestSequence("latest.a"));
        assertEquals(1, events.latestSequence("latest.b"));
        assertThrows(IllegalArgumentException.class, () -> events.latestSequence(""));
    }
}
