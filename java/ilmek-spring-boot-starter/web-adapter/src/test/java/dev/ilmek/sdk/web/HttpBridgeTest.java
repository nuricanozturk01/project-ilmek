package dev.ilmek.sdk.web;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import java.io.IOException;
import java.util.stream.Collectors;
import java.util.stream.StreamSupport;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.springframework.context.ConfigurableApplicationContext;

class HttpBridgeTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();
    private static final String PRINCIPAL = "principal-1";
    private static final String HASH = "a".repeat(64);

    private static ConfigurableApplicationContext context;
    private static IlmekMethodRegistry registry;

    @BeforeAll
    static void start() {
        context = TestRuntime.start(TestRuntime.NO_CONFIG, status -> { });
        registry = context.getBean(IlmekMethodRegistry.class);
    }

    @AfterAll
    static void stop() {
        context.close();
    }

    @Test
    void registersReadAndWriteMethods() {
        assertEquals(MethodKind.READ, registry.find("http.read").orElseThrow().kind());
        assertEquals(MethodKind.WRITE, registry.find("http.write").orElseThrow().kind());
    }

    @Test
    void environmentPostProcessorIsRegisteredAndAppliesNoServerPort() {
        assertEquals("-1", context.getEnvironment().getProperty("server.port"));
        assertTrue(context.getEnvironment().getPropertySources().contains(IlmekNoServerPortPostProcessor.SOURCE_NAME));
    }

    @Test
    void getListAndGetOne() throws IOException {
        JsonNode created = write("{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"one\"}}");
        int id = created.get("body").get("id").asInt();

        JsonNode list = read("{\"method\":\"GET\",\"path\":\"/api/tasks\"}");
        assertEquals(200, list.get("status").asInt());
        assertEquals("application/json", mediaType(list));
        assertTrue(StreamSupport.stream(list.get("body").spliterator(), false)
                .anyMatch(task -> task.get("id").asInt() == id));

        JsonNode one = read("{\"method\":\"GET\",\"path\":\"/api/tasks/" + id + "\"}");
        assertEquals("one", one.get("body").get("title").asText());
    }

    @Test
    void postCreatesWith201AndLocation() throws IOException {
        JsonNode created = write("{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"x\"}}");
        assertEquals(201, created.get("status").asInt());
        assertEquals("/api/tasks/" + created.get("body").get("id").asInt(),
                created.get("headers").get("location").asText());
    }

    @Test
    void validationErrorIs400AndNotAnIpcError() throws IOException {
        JsonNode result = write("{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"\"}}");
        assertEquals(400, result.get("status").asInt());
    }

    @Test
    void putThenDelete() throws IOException {
        int id = write("{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"a\"}}")
                .get("body").get("id").asInt();
        JsonNode replaced = write("{\"method\":\"PUT\",\"path\":\"/api/tasks/" + id + "\",\"body\":{\"title\":\"b\"}}");
        assertEquals(200, replaced.get("status").asInt());
        assertEquals("b", replaced.get("body").get("title").asText());

        JsonNode deleted = write("{\"method\":\"DELETE\",\"path\":\"/api/tasks/" + id + "\"}");
        assertEquals(204, deleted.get("status").asInt());
        assertFalse(deleted.has("body"));
        assertEquals(404, read("{\"method\":\"GET\",\"path\":\"/api/tasks/" + id + "\"}").get("status").asInt());
    }

    @Test
    void patchIsAcceptedByTheWriteContractAndUnmappedVerbIs405() throws IOException {
        assertEquals(405, write("{\"method\":\"PATCH\",\"path\":\"/api/tasks/1\",\"body\":{}}").get("status").asInt());
    }

    @Test
    void headAndOptionsAreServed() throws IOException {
        JsonNode head = read("{\"method\":\"HEAD\",\"path\":\"/api/tasks\"}");
        assertEquals(200, head.get("status").asInt());
        assertFalse(head.has("body"));
        assertEquals(200, read("{\"method\":\"OPTIONS\",\"path\":\"/api/tasks\"}").get("status").asInt());
    }

    @Test
    void unknownPathIs404() throws IOException {
        assertEquals(404, read("{\"method\":\"GET\",\"path\":\"/api/nothing\"}").get("status").asInt());
    }

    @Test
    void thrownControllerExceptionIs500NotAnIpcError() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/boom\"}");
        assertEquals(500, result.get("status").asInt());
        assertEquals("internal server error", result.get("body").get("error").asText());
    }

    @Test
    void queryParametersIncludingRepeatedKeysAndEscaping() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/search\","
                + "\"query\":{\"tag\":[\"a\",\"b\"],\"q\":[\"x y&z=1 ç\"]}}");
        assertEquals(200, result.get("status").asInt());
        JsonNode body = result.get("body");
        assertEquals("a,b", StreamSupport.stream(body.get("tag").spliterator(), false)
                .map(JsonNode::asText).collect(Collectors.joining(",")));
        assertEquals("x y&z=1 ç", body.get("q").asText());
    }

    @Test
    void textResponseIsReturnedAsText() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/text\"}");
        assertEquals("merhaba çay", result.get("text").asText());
        assertFalse(result.has("body"));
        assertTrue(mediaType(result).startsWith("text/plain"));
    }

    @Test
    void binaryResponseIs415() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/binary\"}");
        assertEquals(415, result.get("status").asInt());
        assertEquals("binary responses are not supported", result.get("body").get("error").asText());
    }

    @Test
    void oversizedResponseIs413() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/big\"}");
        assertEquals(413, result.get("status").asInt());
        assertEquals("payload too large", result.get("body").get("error").asText());
    }

    @Test
    void oversizedRequestIs413WithoutCallingTheController() throws IOException {
        String title = "y".repeat(HttpBridge.MAX_PAYLOAD_BYTES);
        JsonNode result = write("{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"" + title + "\"}}");
        assertEquals(413, result.get("status").asInt());
        assertEquals("payload too large", result.get("body").get("error").asText());
    }

    @Test
    void responseHeadersAreLimitedToTheAllowlist() throws IOException {
        JsonNode headers = read("{\"method\":\"GET\",\"path\":\"/api/response-headers\"}").get("headers");
        assertEquals("\"v1\"", headers.get("etag").asText());
        assertEquals("no-cache", headers.get("cache-control").asText());
        assertEquals("/api/tasks/9", headers.get("location").asText());
        assertEquals("application/json", headers.get("content-type").asText());
        assertEquals(4, headers.size());
    }

    @Test
    void requestHeadersAreForwardedExceptTheDroppedOnes() throws IOException {
        JsonNode body = read("{\"method\":\"GET\",\"path\":\"/api/received-headers\",\"headers\":{"
                + "\"cookie\":\"s=1\",\"authorization\":\"Bearer t\",\"host\":\"evil.example\","
                + "\"content-length\":\"999\",\"x-custom\":\"kept\"}}").get("body");
        assertTrue(body.get("cookie").isNull());
        assertTrue(body.get("authorization").isNull());
        assertTrue(body.get("host").isNull() || !body.get("host").asText().contains("evil"));
        assertEquals("kept", body.get("x-custom").asText());
    }

    @Test
    void principalHeaderCarriesTheHostPrincipalAndSpoofedValuesAreStripped() throws IOException {
        JsonNode plain = read("{\"method\":\"GET\",\"path\":\"/api/whoami\"}").get("body");
        assertEquals(PRINCIPAL, plain.get("principal").asText());

        JsonNode spoofed = read("{\"method\":\"GET\",\"path\":\"/api/whoami\","
                + "\"headers\":{\"X-Ilmek-Principal\":\"mallory\"}}").get("body");
        assertEquals(PRINCIPAL, spoofed.get("principal").asText());
    }

    @Test
    void applicationFiltersRun() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/filtered\"}");
        assertEquals("no-store", result.get("headers").get("cache-control").asText());
    }

    @Test
    void asyncControllersAreCompleted() throws IOException {
        JsonNode result = read("{\"method\":\"GET\",\"path\":\"/api/async\"}");
        assertEquals(200, result.get("status").asInt());
        assertEquals("done", result.get("body").get("async").asText());
    }

    @Test
    void readWithBodyIsRejected() {
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/api/tasks\",\"body\":{}}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/api/tasks\",\"body\":null}");
    }

    @Test
    void methodOutsideTheKindIsRejected() {
        assertInvalid("http.read", "{\"method\":\"POST\",\"path\":\"/api/tasks\"}");
        assertInvalid("http.write", "{\"method\":\"GET\",\"path\":\"/api/tasks\"}");
        assertInvalid("http.read", "{\"method\":\"get\",\"path\":\"/api/tasks\"}");
    }

    @Test
    void badPathsAreRejected() {
        for (String path : new String[] {"api/tasks", "", "//host/api", "http://host/api", "/api/tasks?x=1",
                "/api#frag", "/api/a b", "/api\\\\x"}) {
            assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"" + path + "\"}");
        }
    }

    @Test
    void malformedRequestsAreRejected() {
        assertInvalid("http.read", "not json");
        assertInvalid("http.read", "[]");
        assertInvalid("http.read", "{\"method\":\"GET\"}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"extra\":1}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"path\":\"/b\"}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\"} {}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"query\":{\"k\":\"v\"}}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"query\":{\"k\":[1]}}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"headers\":{\"x\":1}}");
        assertInvalid("http.read", "{\"method\":\"GET\",\"path\":\"/a\",\"headers\":{\"bad name\":\"v\"}}");
    }

    private static JsonNode read(String request) throws IOException {
        return invoke("http.read", request);
    }

    private static JsonNode write(String request) throws IOException {
        return invoke("http.write", request);
    }

    private static JsonNode invoke(String method, String request) throws IOException {
        IlmekCallContext call = new IlmekCallContext("app", PRINCIPAL, "http.write".equals(method) ? "op" : null, HASH);
        byte[] result = registry.find(method).orElseThrow().invoke().apply(call, request.getBytes(UTF_8));
        return MAPPER.readTree(result);
    }

    private static void assertInvalid(String method, String request) {
        AppException error = assertThrows(AppException.class, () -> invoke(method, request), request);
        assertEquals(ErrorCode.INVALID_MESSAGE, error.code());
    }

    private static String mediaType(JsonNode result) {
        return result.get("headers").get("content-type").asText();
    }
}
