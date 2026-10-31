package dev.ilmek.examples.tasks;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethod;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import java.io.IOException;
import org.springframework.data.domain.Sort;
import org.springframework.stereotype.Service;

@Service
public class TaskService {

    static final String EVENTS_TOPIC = "tasks.events";

    private static final int MAX_TITLE_CHARS = 200;

    private final TaskRepository tasks;
    private final IlmekEventsService events;
    private final ObjectMapper json = new ObjectMapper();

    public TaskService(TaskRepository tasks, IlmekEventsService events) {
        this.tasks = tasks;
        this.events = events;
    }

    @IlmekMethod(name = "tasks.list", kind = MethodKind.READ)
    public byte[] list(IlmekCallContext context, byte[] body) {
        ObjectNode response = json.createObjectNode();
        ArrayNode items = response.putArray("tasks");
        tasks.findAll(Sort.by("id")).forEach(task -> items.add(view(task)));
        return bytes(response);
    }

    @IlmekMethod(name = "tasks.create", kind = MethodKind.WRITE)
    public byte[] create(IlmekCallContext context, byte[] body) {
        JsonNode title = parse(body).path("title");
        if (!title.isTextual() || title.asText().isBlank() || title.asText().length() > MAX_TITLE_CHARS) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "title must be 1 to 200 characters", false);
        }
        Task task = tasks.save(new Task(title.asText().strip()));
        events.append(EVENTS_TOPIC, event("created", task));
        return bytes(view(task));
    }

    @IlmekMethod(name = "tasks.complete", kind = MethodKind.WRITE)
    public byte[] complete(IlmekCallContext context, byte[] body) {
        JsonNode id = parse(body).path("id");
        if (!id.isIntegralNumber()) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "id must be an integer", false);
        }
        Task task = tasks.findById(id.asLong())
                .orElseThrow(() -> new AppException(ErrorCode.NOT_FOUND, "task not found", false));
        task.complete();
        events.append(EVENTS_TOPIC, event("completed", task));
        return bytes(view(task));
    }

    private ObjectNode view(Task task) {
        ObjectNode node = json.createObjectNode();
        node.put("id", task.getId());
        node.put("title", task.getTitle());
        node.put("done", task.isDone());
        return node;
    }

    private byte[] event(String type, Task task) {
        ObjectNode node = json.createObjectNode();
        node.put("type", type);
        node.set("task", view(task));
        return bytes(node);
    }

    private JsonNode parse(byte[] body) {
        try {
            JsonNode node = json.readTree(body);
            if (node == null || !node.isObject()) {
                throw new AppException(ErrorCode.INVALID_MESSAGE, "body must be a JSON object", false);
            }
            return node;
        } catch (IOException e) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "body must be a JSON object", false);
        }
    }

    private byte[] bytes(JsonNode node) {
        try {
            return json.writeValueAsBytes(node);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }
}
