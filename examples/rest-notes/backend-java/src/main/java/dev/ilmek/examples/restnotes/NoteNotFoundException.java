package dev.ilmek.examples.restnotes;

public class NoteNotFoundException extends RuntimeException {

    public NoteNotFoundException(Long id) {
        super("note " + id + " not found");
    }
}
