package dev.ilmek.sdk.store;

import java.util.List;
import org.springframework.data.domain.Pageable;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Modifying;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

public interface IlmekEventRepository extends JpaRepository<IlmekEvent, Long> {

    @Query("select coalesce(max(e.sequence), 0) from IlmekEvent e where e.topic = :topic")
    long maxSequence(@Param("topic") String topic);

    List<IlmekEvent> findByTopicOrderBySequenceAsc(String topic);

    List<IlmekEvent> findByTopicAndSequenceGreaterThanOrderBySequenceAsc(String topic, long sequence);

    List<IlmekEvent> findByTopicAndSequenceGreaterThanOrderBySequenceAsc(String topic, long sequence,
                                                                         Pageable pageable);

    long countByTopic(String topic);

    @Modifying
    @Query("delete from IlmekEvent e where e.topic = :topic and e.sequence <= :sequence")
    int deleteByTopicUpToSequence(@Param("topic") String topic, @Param("sequence") long sequence);
}
