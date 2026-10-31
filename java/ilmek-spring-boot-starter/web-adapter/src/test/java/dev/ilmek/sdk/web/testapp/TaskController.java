package dev.ilmek.sdk.web.testapp;

import jakarta.validation.Valid;
import jakarta.validation.constraints.NotBlank;
import java.net.URI;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api")
public class TaskController {

    public record TaskInput(@NotBlank String title) {
    }

    public record Task(int id, String title) {
    }

    private final Map<Integer, Task> tasks = new ConcurrentHashMap<>();
    private final AtomicInteger ids = new AtomicInteger();
    private final AtomicInteger creations = new AtomicInteger();

    public int creations() {
        return creations.get();
    }

    @GetMapping("/tasks")
    List<Task> list() {
        return new ArrayList<>(tasks.values());
    }

    @GetMapping("/tasks/{id}")
    Task one(@PathVariable int id) {
        Task task = tasks.get(id);
        if (task == null) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND);
        }
        return task;
    }

    @PostMapping("/tasks")
    ResponseEntity<Task> create(@Valid @RequestBody TaskInput input) {
        creations.incrementAndGet();
        Task task = new Task(ids.incrementAndGet(), input.title());
        tasks.put(task.id(), task);
        return ResponseEntity.created(URI.create("/api/tasks/" + task.id())).body(task);
    }

    @PutMapping("/tasks/{id}")
    Task replace(@PathVariable int id, @Valid @RequestBody TaskInput input) {
        one(id);
        Task task = new Task(id, input.title());
        tasks.put(id, task);
        return task;
    }

    @DeleteMapping("/tasks/{id}")
    ResponseEntity<Void> delete(@PathVariable int id) {
        one(id);
        tasks.remove(id);
        return ResponseEntity.noContent().build();
    }

    @GetMapping(value = "/text", produces = MediaType.TEXT_PLAIN_VALUE)
    String text() {
        return "merhaba çay";
    }

    @GetMapping(value = "/binary", produces = MediaType.APPLICATION_OCTET_STREAM_VALUE)
    byte[] binary() {
        return new byte[] {1, 2, 3};
    }

    @GetMapping("/whoami")
    Map<String, String> whoami(@RequestHeader(name = "X-Ilmek-Principal", required = false) List<String> principal) {
        Map<String, String> result = new LinkedHashMap<>();
        result.put("principal", principal == null ? null : String.join("|", principal));
        return result;
    }

    @GetMapping("/search")
    Map<String, Object> search(@RequestParam List<String> tag, @RequestParam(defaultValue = "") String q) {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("tag", tag);
        result.put("q", q);
        return result;
    }

    @GetMapping("/received-headers")
    Map<String, String> receivedHeaders(@RequestHeader HttpHeaders headers) {
        Map<String, String> result = new LinkedHashMap<>();
        for (String name : List.of("cookie", "authorization", "host", "x-custom", "x-ilmek-principal")) {
            result.put(name, headers.getFirst(name));
        }
        return result;
    }

    @GetMapping("/response-headers")
    ResponseEntity<Map<String, String>> responseHeaders() {
        return ResponseEntity.ok()
                .eTag("\"v1\"")
                .header(HttpHeaders.CACHE_CONTROL, "no-cache")
                .header(HttpHeaders.LOCATION, "/api/tasks/9")
                .header("X-Internal", "secret")
                .body(Map.of("ok", "true"));
    }

    @GetMapping("/boom")
    String boom() {
        throw new IllegalStateException("boom");
    }

    @GetMapping("/big")
    Map<String, String> big() {
        return Map.of("data", "x".repeat(300 * 1024));
    }

    @GetMapping("/filtered")
    Map<String, String> filtered() {
        return Map.of("filtered", "yes");
    }

    @GetMapping("/async")
    Callable<Map<String, String>> async() {
        return () -> Map.of("async", "done");
    }
}
