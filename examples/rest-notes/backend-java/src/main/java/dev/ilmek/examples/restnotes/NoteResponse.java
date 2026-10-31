package dev.ilmek.examples.restnotes;

public record NoteResponse(Long id, String title, String body) {

    static NoteResponse of(Note note) {
        return new NoteResponse(note.getId(), note.getTitle(), note.getBody());
    }
}
