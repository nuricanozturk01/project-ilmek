package dev.ilmek.examples.restnotes;

import jakarta.validation.Valid;
import java.net.URI;
import java.util.List;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/notes")
public class NotesController {

    private final NoteRepository notes;

    public NotesController(NoteRepository notes) {
        this.notes = notes;
    }

    @GetMapping
    public List<NoteResponse> list(@RequestParam(name = "q", required = false) String q) {
        List<Note> found = q == null || q.isBlank()
                ? notes.findAllByOrderByIdAsc()
                : notes.findByTitleContainingIgnoreCaseOrBodyContainingIgnoreCaseOrderByIdAsc(q, q);
        return found.stream().map(NoteResponse::of).toList();
    }

    @GetMapping("/{id}")
    public NoteResponse get(@PathVariable Long id) {
        return NoteResponse.of(notes.findById(id).orElseThrow(() -> new NoteNotFoundException(id)));
    }

    @PostMapping
    public ResponseEntity<NoteResponse> create(@Valid @RequestBody NoteRequest request) {
        Note saved = notes.save(new Note(request.title(), bodyOf(request)));
        return ResponseEntity.created(URI.create("/api/notes/" + saved.getId())).body(NoteResponse.of(saved));
    }

    @PutMapping("/{id}")
    public NoteResponse replace(@PathVariable Long id, @Valid @RequestBody NoteRequest request) {
        Note note = notes.findById(id).orElseThrow(() -> new NoteNotFoundException(id));
        note.update(request.title(), bodyOf(request));
        return NoteResponse.of(notes.save(note));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> delete(@PathVariable Long id) {
        if (!notes.existsById(id)) {
            throw new NoteNotFoundException(id);
        }
        notes.deleteById(id);
        return ResponseEntity.noContent().build();
    }

    private static String bodyOf(NoteRequest request) {
        return request.body() == null ? "" : request.body();
    }
}
