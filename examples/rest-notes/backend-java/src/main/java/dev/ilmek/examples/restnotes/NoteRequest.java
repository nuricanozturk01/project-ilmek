package dev.ilmek.examples.restnotes;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record NoteRequest(
        @NotBlank @Size(min = 1, max = 100) String title,
        @Size(max = 2000) String body) {
}
