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
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
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
        assertEquals(0, readObject(invoke("tasks.list", null, EMPTY)).get("tasks").size());

        createTask(OP_CREATE, "Buy milk");

        JsonNode list = readObject(invoke("tasks.list", null, EMPTY)).get("tasks");
        assertEquals(1, list.size());
        assertEquals("Buy milk", list.get(0).get("title").asText());
        assertFalse(list.get(0).get("done").asBoolean());
    }

    @Test
    void createPersistsRowAndAppendsOneEvent() {
        JsonNode created = createTask(OP_CREATE, "Buy milk");

        assertEquals("Buy milk", created.get("title").asText());
        assertFalse(created.get("done").asBoolean());
        assertEquals(1, tasks.count());
        assertEquals(1, events.countForTopic("tasks.events"));

        IlmekEvent event = events.since("tasks.events", null).get(0);
        JsonNode eventBody = readObject(event.getBody());
        assertEquals("created", eventBody.get("type").asText());
        assertEquals(created.get("id").asLong(), eventBody.get("task").get("id").asLong());
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
        long id = createTask(OP_CREATE, "Buy milk").get("id").asLong();

        JsonNode completed = readObject(write("tasks.complete", OP_COMPLETE, body("{\"id\":" + id + "}")));

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

    private JsonNode createTask(String operationId, String title) {
        return readObject(write("tasks.create", operationId, body("{\"title\":\"" + title + "\"}")));
    }

    private byte[] write(String method, String operationId, byte[] body) {
        IlmekMethodRegistry.MethodHandle handle = registry.find(method).orElseThrow();
        assertEquals(MethodKind.WRITE, handle.kind());
        IlmekCallContext context = new IlmekCallContext(APP, PRINCIPAL, operationId, payloadHash(method, body));
        return operations.write(PRINCIPAL, operationId, method, context.payloadHash(),
                () -> handle.invoke().apply(context, body));
    }

    private byte[] invoke(String method, String operationId, byte[] body) {
        IlmekMethodRegistry.MethodHandle handle = registry.find(method).orElseThrow();
        assertEquals(MethodKind.READ, handle.kind());
        return handle.invoke().apply(new IlmekCallContext(APP, PRINCIPAL, operationId, null), body);
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
        } catch (java.security.NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    private static byte[] body(String json) {
        return json.getBytes(UTF_8);
    }

    private static JsonNode readObject(byte[] bytes) {
        try {
            return JSON.readTree(bytes);
        } catch (java.io.IOException e) {
            throw new AssertionError("response is not JSON", e);
        }
    }
}
