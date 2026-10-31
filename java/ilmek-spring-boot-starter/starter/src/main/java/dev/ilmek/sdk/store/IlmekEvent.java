package dev.ilmek.sdk.store;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import jakarta.persistence.UniqueConstraint;

@Entity
@Table(name = "ilmek_events", uniqueConstraints = @UniqueConstraint(columnNames = {"topic", "sequence"}))
public class IlmekEvent {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(name = "topic", nullable = false)
    private String topic;

    @Column(name = "sequence", nullable = false)
    private long sequence;

    @Column(name = "event_id", nullable = false, length = 36)
    private String eventId;

    @Column(name = "body", nullable = false, length = 1_048_576)
    private byte[] body;

    @Column(name = "created_at_ms", nullable = false)
    private long createdAtMs;

    protected IlmekEvent() {
    }

    public IlmekEvent(String topic, long sequence, String eventId, byte[] body, long createdAtMs) {
        this.topic = topic;
        this.sequence = sequence;
        this.eventId = eventId;
        this.body = body;
        this.createdAtMs = createdAtMs;
    }

    public Long getId() {
        return id;
    }

    public String getTopic() {
        return topic;
    }

    public long getSequence() {
        return sequence;
    }

    public String getEventId() {
        return eventId;
    }

    public byte[] getBody() {
        return body;
    }

    public long getCreatedAtMs() {
        return createdAtMs;
    }
}
