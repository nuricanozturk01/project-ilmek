package dev.ilmek.sdk.store;

import java.util.List;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class IlmekEventsService {

    public static final int RETAINED_PER_TOPIC = 10_000;

    private final IlmekEventRepository repository;
    private final IlmekEventFloorRepository floors;

    public IlmekEventsService(IlmekEventRepository repository, IlmekEventFloorRepository floors) {
        this.repository = repository;
        this.floors = floors;
    }

    @Transactional
    public IlmekEvent append(String topic, byte[] body) {
        requireTopic(topic);
        Objects.requireNonNull(body, "body");
        long sequence = repository.maxSequence(topic) + 1;
        IlmekEvent saved = repository.save(
            new IlmekEvent(topic, sequence, newEventId(), body, System.currentTimeMillis()));
        long trimBound = sequence - RETAINED_PER_TOPIC;
        if (trimBound > 0) {
            repository.deleteByTopicUpToSequence(topic, trimBound);
            raiseFloor(topic, trimBound);
        }
        return saved;
    }

    @Transactional(readOnly = true)
    public List<IlmekEvent> since(String topic, String cursorOrNull) {
        requireTopic(topic);
        if (cursorOrNull == null) {
            return repository.findByTopicOrderBySequenceAsc(topic);
        }
        EventCursor.Parsed cursor = EventCursor.decode(cursorOrNull)
            .orElseThrow(() -> new IllegalArgumentException("invalid cursor"));
        if (!cursor.topic().equals(topic)) {
            throw new IllegalArgumentException("cursor belongs to another topic");
        }
        if (cursor.sequence() < trimFloor(topic)) {
            throw new CursorExpiredException();
        }
        return repository.findByTopicAndSequenceGreaterThanOrderBySequenceAsc(topic, cursor.sequence());
    }

    @Transactional(readOnly = true)
    public long latestSequence(String topic) {
        requireTopic(topic);
        return repository.maxSequence(topic);
    }

    @Transactional(readOnly = true)
    public long countForTopic(String topic) {
        return repository.countByTopic(topic);
    }

    private void raiseFloor(String topic, long sequence) {
        IlmekEventFloor floor = floors.findById(topic).orElseGet(() -> new IlmekEventFloor(topic, 0));
        floor.raiseTo(sequence);
        floors.save(floor);
    }

    private long trimFloor(String topic) {
        return floors.findById(topic).map(IlmekEventFloor::getFloorSequence).orElse(0L);
    }

    private static void requireTopic(String topic) {
        if (topic == null || topic.isEmpty()) {
            throw new IllegalArgumentException("topic is required");
        }
    }

    private static String newEventId() {
        long unixMillis = System.currentTimeMillis();
        long randA = ThreadLocalRandom.current().nextLong() & 0xFFFL;
        long randB = ThreadLocalRandom.current().nextLong() & 0x3FFFFFFFFFFFFFFFL;
        long msb = (unixMillis << 16) | 0x7000L | randA;
        long lsb = 0x8000000000000000L | randB;
        return new UUID(msb, lsb).toString();
    }
}
