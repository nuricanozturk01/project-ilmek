package dev.ilmek.examples.restnotes;

import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;

public interface NoteRepository extends JpaRepository<Note, Long> {

    List<Note> findAllByOrderByIdAsc();

    List<Note> findByTitleContainingIgnoreCaseOrBodyContainingIgnoreCaseOrderByIdAsc(String title, String body);
}
