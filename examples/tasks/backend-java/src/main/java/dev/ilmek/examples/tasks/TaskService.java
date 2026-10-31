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
import java.util.List;
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
        Task task = tasks.save(new Task(title(parse(body))));
        publish(event("created").put("id", task.getId()).put("title", task.getTitle()));
        return taskResponse(task);
    }

    @IlmekMethod(name = "tasks.complete", kind = MethodKind.WRITE)
    public byte[] complete(IlmekCallContext context, byte[] body) {
        JsonNode request = parse(body);
        Task task = find(request);
        boolean done = done(request);
        task.setDone(done);
        publish(event(done ? "completed" : "reopened").put("id", task.getId()));
        return taskResponse(task);
    }

    @IlmekMethod(name = "tasks.update", kind = MethodKind.WRITE)
    public byte[] update(IlmekCallContext context, byte[] body) {
        JsonNode request = parse(body);
        String title = title(request);
        Task task = find(request);
        task.rename(title);
        publish(event("updated").put("id", task.getId()));
        return taskResponse(task);
    }

    @IlmekMethod(name = "tasks.delete", kind = MethodKind.WRITE)
    public byte[] delete(IlmekCallContext context, byte[] body) {
        Task task = find(parse(body));
        tasks.delete(task);
        publish(event("deleted").put("id", task.getId()));
        return bytes(json.createObjectNode().put("deleted", task.getId()));
    }

    @IlmekMethod(name = "tasks.clear_completed", kind = MethodKind.WRITE)
    public byte[] clearCompleted(IlmekCallContext context, byte[] body) {
        parse(body);
        List<Task> completed = tasks.findByDone(true);
        tasks.deleteAll(completed);
        if (!completed.isEmpty()) {
            publish(event("cleared").put("count", completed.size()));
        }
        return bytes(json.createObjectNode().put("removed", completed.size()));
    }

    private Task find(JsonNode request) {
        JsonNode id = request.path("id");
        if (!id.isIntegralNumber()) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "id must be an integer", false);
        }
        return tasks.findById(id.asLong())
                .orElseThrow(() -> new AppException(ErrorCode.NOT_FOUND, "task not found", false));
    }

    private String title(JsonNode request) {
        JsonNode title = request.path("title");
        String text = title.isTextual() ? title.asText().strip() : "";
        if (text.isEmpty() || text.length() > MAX_TITLE_CHARS) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "title must be 1 to 200 characters", false);
        }
        return text;
    }

    private boolean done(JsonNode request) {
        JsonNode done = request.path("done");
        if (done.isMissingNode()) {
            return true;
        }
        if (!done.isBoolean()) {
            throw new AppException(ErrorCode.INVALID_MESSAGE, "done must be a boolean", false);
        }
        return done.booleanValue();
    }

    private byte[] taskResponse(Task task) {
        return bytes(json.createObjectNode().set("task", view(task)));
    }

    private ObjectNode view(Task task) {
        ObjectNode node = json.createObjectNode();
        node.put("id", task.getId());
        node.put("title", task.getTitle());
        node.put("done", task.isDone());
        return node;
    }

    private ObjectNode event(String type) {
        return json.createObjectNode().put("type", type);
    }

    private void publish(ObjectNode event) {
        events.append(EVENTS_TOPIC, bytes(event));
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
