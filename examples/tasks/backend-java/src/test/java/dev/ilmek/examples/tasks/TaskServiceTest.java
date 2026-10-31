package dev.ilmek.examples.tasks;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.store.IlmekEvent;
import dev.ilmek.sdk.store.IlmekEventRepository;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import dev.ilmek.sdk.store.IlmekOperationsService;
import dev.ilmek.sdk.store.OperationConflictException;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import java.io.IOException;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.DynamicPropertyRegistry;
import org.springframework.test.context.DynamicPropertySource;

@SpringBootTest
class TaskServiceTest {

    private static final String APP = "0190f5a0-0000-7000-8000-000000000001";
    private static final String PRINCIPAL = "0190f5a0-0000-7000-8000-000000000002";
    private static final String OP_CREATE = "0190f5a0-0000-7000-8000-0000000000c1";
    private static final String OP_REJECTED = "0190f5a0-0000-7000-8000-0000000000c2";
    private static final String OP_COMPLETE = "0190f5a0-0000-7000-8000-0000000000d1";
    private static final byte[] EMPTY = new byte[0];
    private static final AtomicLong OPERATIONS = new AtomicLong(0xe000);

    private static final ObjectMapper JSON = new ObjectMapper();

    @TempDir
    static Path dataDir;

    @DynamicPropertySource
    static void database(DynamicPropertyRegistry registry) {
        registry.add("spring.datasource.url",
                () -> "jdbc:h2:file:" + dataDir.resolve("tasks-test").toAbsolutePath());
    }

    @Autowired
    private IlmekMethodRegistry registry;

    @Autowired
    private IlmekOperationsService operations;

    @Autowired
    private IlmekEventsService events;

    @Autowired
    private IlmekEventRepository eventRows;

    @Autowired
    private IlmekOperationRepository operationRows;

    @Autowired
    private TaskRepository tasks;

    @BeforeEach
    void clean() {
        eventRows.deleteAll();
        operationRows.deleteAll();
        tasks.deleteAll();
    }

    @Test
    void listReturnsEmptyThenPopulatedTasks() {
        assertEquals(0, readObject(invoke("tasks.list", EMPTY)).get("tasks").size());

        createTask("Buy milk");

        JsonNode list = readObject(invoke("tasks.list", EMPTY)).get("tasks");
        assertEquals(1, list.size());
        assertEquals("Buy milk", list.get(0).get("title").asText());
        assertFalse(list.get(0).get("done").asBoolean());
    }

    @Test
    void createPersistsRowAndAppendsOneEvent() {
        JsonNode created = createTask("Buy milk");

        assertEquals("Buy milk", created.get("title").asText());
        assertFalse(created.get("done").asBoolean());
        assertEquals(1, tasks.count());
        assertEquals(1, events.countForTopic("tasks.events"));

        JsonNode eventBody = events().get(0);
        assertEquals("created", eventBody.get("type").asText());
        assertEquals(created.get("id").asLong(), eventBody.get("id").asLong());
        assertEquals("Buy milk", eventBody.get("title").asText());
        assertFalse(eventBody.has("task"));
    }

    @Test
    void createResponseWrapsTaskInObject() {
        JsonNode response = writeObject("tasks.create", "{\"title\":\"Buy milk\"}");

        assertTrue(response.get("task").get("id").isIntegralNumber());
        assertEquals("Buy milk", response.get("task").get("title").asText());
        assertFalse(response.get("task").get("done").asBoolean());
    }

    @Test
    void createTrimsTitle() {
        assertEquals("Buy milk", createTask("  Buy milk \t").get("title").asText());
    }

    @Test
    void createRejectsBlankAndOversizedTitles() {
        for (String title : List.of("", "   ", "x".repeat(201), " " + "x".repeat(200) + " x")) {
            AppException rejected = assertThrows(AppException.class,
                    () -> write("tasks.create", newOperation(), body(titleBody(title))));
            assertEquals(ErrorCode.INVALID_MESSAGE, rejected.code());
        }
        assertEquals(0, tasks.count());
        assertEquals(0, events().size());
    }

    @Test
    void createAcceptsTwoHundredCharacterTitle() {
        String title = "x".repeat(200);

        assertEquals(title, createTask(title).get("title").asText());
        assertEquals(title, tasks.findAll().get(0).getTitle());
    }

    @Test
    void createEventCarriesTypeIdAndTitle() {
        long id = createTask("Buy milk").get("id").asLong();

        assertEquals(1, events().size());
        assertEquals("created", events().get(0).get("type").asText());
        assertEquals(id, events().get(0).get("id").asLong());
        assertEquals("Buy milk", events().get(0).get("title").asText());
    }

    @Test
    void updateRenamesTaskAndAppendsEvent() {
        long id = createTask("Buy milk").get("id").asLong();

        JsonNode updated = writeObject("tasks.update", "{\"id\":" + id + ",\"title\":\"  Buy bread \"}").get("task");

        assertEquals(id, updated.get("id").asLong());
        assertEquals("Buy bread", updated.get("title").asText());
        assertEquals("Buy bread", tasks.findById(id).orElseThrow().getTitle());
        assertEquals("updated", events().get(1).get("type").asText());
        assertEquals(id, events().get(1).get("id").asLong());
    }

    @Test
    void updateOnUnknownIdReturnsNotFound() {
        AppException missing = assertThrows(AppException.class,
                () -> write("tasks.update", newOperation(), body("{\"id\":999999,\"title\":\"Buy bread\"}")));

        assertEquals(ErrorCode.NOT_FOUND, missing.code());
        assertEquals(0, events().size());
    }

    @Test
    void updateRejectsBlankTitle() {
        long id = createTask("Buy milk").get("id").asLong();

        AppException rejected = assertThrows(AppException.class,
                () -> write("tasks.update", newOperation(), body("{\"id\":" + id + ",\"title\":\"  \"}")));

        assertEquals(ErrorCode.INVALID_MESSAGE, rejected.code());
        assertEquals("Buy milk", tasks.findById(id).orElseThrow().getTitle());
        assertEquals(1, events().size());
    }

    @Test
    void completeWithoutDoneFieldMarksTaskDone() {
        long id = createTask("Buy milk").get("id").asLong();

        JsonNode completed = writeObject("tasks.complete", "{\"id\":" + id + "}").get("task");

        assertTrue(completed.get("done").asBoolean());
        assertEquals("completed", events().get(1).get("type").asText());
        assertEquals(id, events().get(1).get("id").asLong());
    }

    @Test
    void completeWithDoneFalseReopensTask() {
        long id = createTask("Buy milk").get("id").asLong();
        writeObject("tasks.complete", "{\"id\":" + id + ",\"done\":true}");

        JsonNode reopened = writeObject("tasks.complete", "{\"id\":" + id + ",\"done\":false}").get("task");

        assertFalse(reopened.get("done").asBoolean());
        assertFalse(tasks.findById(id).orElseThrow().isDone());
        assertEquals("reopened", events().get(2).get("type").asText());
        assertEquals(id, events().get(2).get("id").asLong());
    }

    @Test
    void deleteRemovesRowAndAppendsEvent() {
        long id = createTask("Buy milk").get("id").asLong();

        JsonNode deleted = writeObject("tasks.delete", "{\"id\":" + id + "}");

        assertEquals(id, deleted.get("deleted").asLong());
        assertEquals(0, tasks.count());
        assertEquals("deleted", events().get(1).get("type").asText());
        assertEquals(id, events().get(1).get("id").asLong());
    }

    @Test
    void deleteOnUnknownIdReturnsNotFound() {
        AppException missing = assertThrows(AppException.class,
                () -> write("tasks.delete", newOperation(), body("{\"id\":999999}")));

        assertEquals(ErrorCode.NOT_FOUND, missing.code());
        assertEquals(0, events().size());
    }

    @Test
    void clearCompletedRemovesOnlyDoneTasksAndReturnsCount() {
        long open = createTask("Keep me").get("id").asLong();
        long first = createTask("Done one").get("id").asLong();
        long second = createTask("Done two").get("id").asLong();
        writeObject("tasks.complete", "{\"id\":" + first + "}");
        writeObject("tasks.complete", "{\"id\":" + second + "}");

        JsonNode result = writeObject("tasks.clear_completed", "{}");

        assertEquals(2, result.get("removed").asInt());
        assertEquals(List.of(open), tasks.findAll().stream().map(Task::getId).toList());
        JsonNode last = events().get(events().size() - 1);
        assertEquals("cleared", last.get("type").asText());
        assertEquals(2, last.get("count").asInt());
    }

    @Test
    void clearCompletedWithNothingDoneAppendsNoEvent() {
        createTask("Open");
        int before = events().size();

        JsonNode result = writeObject("tasks.clear_completed", "{}");

        assertEquals(0, result.get("removed").asInt());
        assertEquals(before, events().size());
        assertEquals(1, tasks.count());
    }

    @Test
    void createRetriedWithSameOperationReplaysWithoutSecondRow() {
        byte[] body = body("{\"title\":\"Buy milk\"}");

        byte[] first = write("tasks.create", OP_CREATE, body);
        byte[] second = write("tasks.create", OP_CREATE, body);

        assertArrayEquals(first, second);
        assertEquals(1, tasks.count());
        assertEquals(1, events.countForTopic("tasks.events"));
        assertEquals(1, operationRows.count());
    }

    @Test
    void createRetriedWithSameOperationIdAndDifferentBodyIsRejected() {
        write("tasks.create", OP_CREATE, body("{\"title\":\"Buy milk\"}"));

        AppException conflict = assertThrows(OperationConflictException.class,
                () -> write("tasks.create", OP_CREATE, body("{\"title\":\"Buy bread\"}")));

        assertEquals(ErrorCode.OPERATION_CONFLICT, conflict.code());
        assertEquals(1, tasks.count());
        assertEquals(1, events.countForTopic("tasks.events"));
    }

    @Test
    void createRejectsMissingTitleWithoutWriting() {
        AppException rejected = assertThrows(AppException.class,
                () -> write("tasks.create", OP_REJECTED, body("{\"name\":\"Buy milk\"}")));

        assertEquals(ErrorCode.INVALID_MESSAGE, rejected.code());
        assertEquals(0, tasks.count());
        assertEquals(0, events.countForTopic("tasks.events"));
    }

    @Test
    void completeMarksTaskDoneAndAppendsEvent() {
        long id = createTask("Buy milk").get("id").asLong();

        JsonNode completed = writeObject("tasks.complete", "{\"id\":" + id + "}").get("task");

        assertTrue(completed.get("done").asBoolean());
        assertTrue(tasks.findById(id).orElseThrow().isDone());
        assertEquals(2, events.countForTopic("tasks.events"));
    }

    @Test
    void completeOnUnknownIdReturnsNotFound() {
        AppException missing = assertThrows(AppException.class,
                () -> write("tasks.complete", OP_COMPLETE, body("{\"id\":999999}")));

        assertEquals(ErrorCode.NOT_FOUND, missing.code());
        assertEquals(0, events.countForTopic("tasks.events"));
    }

    private JsonNode createTask(String title) {
        return writeObject("tasks.create", titleBody(title)).get("task");
    }

    private JsonNode writeObject(String method, String json) {
        return readObject(write(method, newOperation(), body(json)));
    }

    private List<JsonNode> events() {
        return events.since("tasks.events", null).stream()
                .map(event -> readObject(event.getBody()))
                .toList();
    }

    private static String newOperation() {
        return String.format("0190f5a0-0000-7000-8000-%012x", OPERATIONS.incrementAndGet());
    }

    private static String titleBody(String title) {
        return JSON.createObjectNode().put("title", title).toString();
    }

    private byte[] write(String method, String operationId, byte[] body) {
        IlmekMethodRegistry.MethodHandle handle = registry.find(method).orElseThrow();
        assertEquals(MethodKind.WRITE, handle.kind());
        IlmekCallContext context = new IlmekCallContext(APP, PRINCIPAL, operationId, payloadHash(method, body));
        return operations.write(PRINCIPAL, operationId, method, context.payloadHash(),
                () -> handle.invoke().apply(context, body));
    }

    private byte[] invoke(String method, byte[] body) {
        IlmekMethodRegistry.MethodHandle handle = registry.find(method).orElseThrow();
        assertEquals(MethodKind.READ, handle.kind());
        return handle.invoke().apply(new IlmekCallContext(APP, PRINCIPAL, null, null), body);
    }

    private static String payloadHash(String method, byte[] body) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            digest.update("ILMEK-CALL-V1".getBytes(UTF_8));
            digest.update((byte) 0);
            digest.update(APP.getBytes(UTF_8));
            digest.update((byte) 0);
            digest.update(method.getBytes(UTF_8));
            digest.update((byte) 0);
            digest.update(body);
            return HexFormat.of().formatHex(digest.digest());
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static byte[] body(String json) {
        return json.getBytes(UTF_8);
    }

    private static JsonNode readObject(byte[] bytes) {
        try {
            return JSON.readTree(bytes);
        } catch (IOException e) {
            throw new AssertionError("response is not JSON", e);
        }
    }
}
