package dev.ilmek.sdk.autoconfigure.app;

import org.springframework.data.jpa.repository.JpaRepository;

public interface AppNoteRepository extends JpaRepository<AppNote, Long> {
}
