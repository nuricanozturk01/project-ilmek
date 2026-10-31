package dev.ilmek.sdk.wire;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.ByteBuffer;
import org.junit.jupiter.api.Test;

class StrictJsonTest {

    private static final String VALID =
            "{\"type\":\"IPC_HELLO\",\"request_id\":\"r1\",\"community_id\":\"c1\",\"data\":{\"x\":1}}";

    private static byte[] bytes(String json) {
        return json.getBytes(UTF_8);
    }

    @Test
    void parsesAValidEnvelope() {
        Envelope e = StrictJson.parseEnvelope("""
            {"type":"IPC_HELLO","request_id":"r1","community_id":"c1","data":{"x":1}}
            """.getBytes(UTF_8));
        assertEquals("IPC_HELLO", e.type());
        assertEquals("r1", e.requestId());
        assertEquals("c1", e.communityId());
        assertEquals(1, e.data().get("x").asInt());
    }

    @Test
    void acceptsFieldsInAnyOrderAndKeepsDataTree() {
        Envelope e = StrictJson.parseEnvelope(bytes(
                "{\"data\":{\"nested\":{\"k\":[1,2,3]}},\"community_id\":\"c\",\"request_id\":\"r\",\"type\":\"PING\"}"));
        assertEquals("PING", e.type());
        assertEquals(3, e.data().get("nested").get("k").size());
    }

    @Test
    void rejectsDuplicateTopLevelKey() {
        byte[] json = "{\"type\":\"A\",\"type\":\"B\",\"request_id\":\"r\",\"community_id\":\"c\",\"data\":{}}".getBytes(UTF_8);
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsDuplicateKeyInsideData() {
        byte[] json = bytes("{\"type\":\"A\",\"request_id\":\"r\",\"community_id\":\"c\",\"data\":{\"k\":1,\"k\":2}}");
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsUnknownTopLevelField() {
        byte[] json = bytes("{\"type\":\"A\",\"request_id\":\"r\",\"community_id\":\"c\",\"data\":{},\"extra\":1}");
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsMissingField() {
        byte[] json = bytes("{\"type\":\"A\",\"request_id\":\"r\",\"data\":{}}");
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsLeadingBom() {
        ByteBuffer buf = ByteBuffer.allocate(3 + VALID.length());
        buf.put(new byte[] {(byte) 0xEF, (byte) 0xBB, (byte) 0xBF}).put(bytes(VALID));
        byte[] json = buf.array();
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsInvalidUtf8() {
        byte[] json = bytes("{\"type\":\"A\",\"request_id\":\"r\",\"community_id\":\"c\",\"data\":{\"s\":\"\"}}");
        json[json.length - 4] = (byte) 0xC3;
        json[json.length - 3] = (byte) 0x28;
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(json));
    }

    @Test
    void rejectsNonStringIdentityFields() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(
                "{\"type\":1,\"request_id\":\"r\",\"community_id\":\"c\",\"data\":{}}")));
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(
                "{\"type\":\"A\",\"request_id\":null,\"community_id\":\"c\",\"data\":{}}")));
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(
                "{\"type\":\"A\",\"request_id\":\"r\",\"community_id\":[],\"data\":{}}")));
    }

    @Test
    void rejectsNonObjectData() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(
                "{\"type\":\"A\",\"request_id\":\"r\",\"community_id\":\"c\",\"data\":[1]}")));
    }

    @Test
    void rejectsTopLevelThatIsNotAnObject() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes("[]")));
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes("\"x\"")));
    }

    @Test
    void rejectsTrailingContentAfterObject() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(VALID + "{}")));
    }

    @Test
    void rejectsTruncatedJson() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(bytes(VALID.substring(0, 30))));
    }

    @Test
    void rejectsEmptyInput() {
        assertThrows(StrictJsonException.class, () -> StrictJson.parseEnvelope(new byte[0]));
    }
}
