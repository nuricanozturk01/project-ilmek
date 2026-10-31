package dev.ilmek.sdk.store;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

@Entity
@Table(name = "ilmek_event_floors")
public class IlmekEventFloor {

    @Id
    @Column(name = "topic", nullable = false)
    private String topic;

    @Column(name = "floor_sequence", nullable = false)
    private long floorSequence;

    protected IlmekEventFloor() {
    }

    public IlmekEventFloor(String topic, long floorSequence) {
        this.topic = topic;
        this.floorSequence = floorSequence;
    }

    public String getTopic() {
        return topic;
    }

    public long getFloorSequence() {
        return floorSequence;
    }

    void raiseTo(long sequence) {
        if (sequence > floorSequence) {
            floorSequence = sequence;
        }
    }
}
