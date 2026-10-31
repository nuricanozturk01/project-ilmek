package dev.ilmek.sdk.wire;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import dev.ilmek.sdk.wire.IpcMessage.Body;
import java.util.Map;
import org.junit.jupiter.api.Test;

class IpcCodecTest {

    private static final String REQUEST_ID = "01970000-0000-7000-8000-0000000000a1";
    private static final String COMMUNITY_ID = "01970000-0000-7000-8000-0000000000c1";
    private static final String APP_ID = "01970000-0000-7000-8000-000000000001";
    private static final String OP_ID = "01970000-0000-7000-8000-0000000000b1";
    private static final String PRINCIPAL_ID = "01970000-0000-7000-8000-0000000000d1";
    private static final String SUB_ID = "01970000-0000-7000-8000-0000000000e1";
    private static final String EVENT_ID = "01970000-0000-7000-8000-0000000000f1";
    private static final String HASH = "ab".repeat(32);
    private static final String CURSOR = "cursor_AB-09";
    private static final String VERSION = "1.2.3";
    private static final String METHOD = "notes.list";
    private static final String TOPIC = "notes.changed";

    private static final Body BODY = new Body("{\"limit\":10}".getBytes(UTF_8));

    private static IpcMessage roundTrip(IpcMessage message) {
        IpcCodec.Decoded decoded = roundTripWithHeader(message);
        return decoded.message();
    }

    private static IpcCodec.Decoded roundTripWithHeader(IpcMessage message) {
        FrameDecoder decoder = new FrameDecoder();
        decoder.push(IpcCodec.encode(message, REQUEST_ID, COMMUNITY_ID));
        Frame frame = decoder.nextFrame().orElseThrow();
        return IpcCodec.decodeWithHeader(frame.metadata(), frame.body());
    }

    private static void assertRoundTrip(IpcMessage message) {
        assertEquals(message, roundTrip(message));
    }

    private static IpcMessage decode(String metadataJson, byte[] body) {
        return IpcCodec.decode(metadataJson.getBytes(UTF_8), body);
    }

    private static String envelope(String type, String data) {
        return "{\"type\":\"" + type + "\",\"request_id\":\"" + REQUEST_ID
                + "\",\"community_id\":\"" + COMMUNITY_ID + "\",\"data\":" + data + "}";
    }

    @Test
    void roundTripsIpcHello() {
        assertRoundTrip(new IpcMessage.Hello("token123", APP_ID));
    }

    @Test
    void roundTripsIpcReady() {
        assertRoundTrip(new IpcMessage.Ready(APP_ID));
    }

    @Test
    void roundTripsIpcCallWithOperationIdAndBody() {
        assertRoundTrip(new IpcMessage.Call(APP_ID, VERSION, METHOD, OP_ID, HASH, PRINCIPAL_ID, BODY));
    }

    @Test
    void roundTripsIpcCallWithoutOperationId() {
        assertRoundTrip(new IpcMessage.Call(APP_ID, VERSION, METHOD, null, HASH, PRINCIPAL_ID, new Body(new byte[0])));
    }

    @Test
    void roundTripsIpcResult() {
        assertRoundTrip(new IpcMessage.Result(APP_ID, OP_ID, BODY));
        assertRoundTrip(new IpcMessage.Result(APP_ID, null, new Body(new byte[0])));
    }

    @Test
    void roundTripsIpcErrorWithRetryAndDetails() {
        IpcMessage.Error error = new IpcMessage.Error(
                ErrorCode.APP_UNAVAILABLE, "backend restarting", true, 250L,
                Map.of("attempt", jsonNumber(2)));
        assertRoundTrip(error);
    }

    @Test
    void roundTripsIpcErrorWithoutRetryAfter() {
        assertRoundTrip(new IpcMessage.Error(ErrorCode.NOT_FOUND, "no such note", false, null, Map.of()));
    }

    @Test
    void roundTripsIpcSubscribeWithCursor() {
        assertRoundTrip(new IpcMessage.Subscribe(APP_ID, VERSION, TOPIC, CURSOR, PRINCIPAL_ID));
    }

    @Test
    void roundTripsIpcSubscribeWithoutCursor() {
        assertRoundTrip(new IpcMessage.Subscribe(APP_ID, VERSION, TOPIC, null, PRINCIPAL_ID));
    }

    @Test
    void roundTripsIpcSubscribed() {
        assertRoundTrip(new IpcMessage.Subscribed(SUB_ID, APP_ID, TOPIC, CURSOR, 64L));
    }

    @Test
    void roundTripsIpcEvent() {
        assertRoundTrip(new IpcMessage.Event(SUB_ID, APP_ID, TOPIC, 7L, EVENT_ID, CURSOR, BODY));
    }

    @Test
    void roundTripsIpcEventAck() {
        assertRoundTrip(new IpcMessage.EventAck(SUB_ID, 7L, CURSOR));
    }

    @Test
    void roundTripsIpcEventAcked() {
        assertRoundTrip(new IpcMessage.EventAcked(SUB_ID, 7L));
    }

    @Test
    void roundTripsIpcUnsubscribe() {
        assertRoundTrip(new IpcMessage.Unsubscribe(SUB_ID));
    }

    @Test
    void roundTripsIpcUnsubscribed() {
        assertRoundTrip(new IpcMessage.Unsubscribed(SUB_ID));
    }

    @Test
    void preservesRequestAndCommunityIdsThroughDecodeWithHeader() {
        IpcCodec.Decoded decoded = roundTripWithHeader(new IpcMessage.Ready(APP_ID));
        assertEquals(REQUEST_ID, decoded.requestId());
        assertEquals(COMMUNITY_ID, decoded.communityId());
        assertEquals(new IpcMessage.Ready(APP_ID), decoded.message());
    }

    @Test
    void preservesBodyBytesExactly() {
        byte[] raw = "{ \"b\" : 1 }".getBytes(UTF_8);
        IpcMessage decoded = roundTrip(new IpcMessage.Result(APP_ID, OP_ID, new Body(raw)));
        assertArrayEquals(raw, ((IpcMessage.Result) decoded).body().bytes());
    }

    @Test
    void rejectsUnknownIpcType() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_BOGUS", "{}"), new byte[0]));
    }

    @Test
    void networkMessageTypeIsRejectedByIpcDecoder() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("HELLO", "{\"token\":\"token123\",\"app_id\":\"" + APP_ID + "\"}"), new byte[0]));
    }

    @Test
    void rejectsBodyOnBodilessMessage() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_READY", "{\"app_id\":\"" + APP_ID + "\"}"), new byte[]{'{', '}'}));
    }

    @Test
    void rejectsUnknownDataField() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_READY", "{\"app_id\":\"" + APP_ID + "\",\"extra\":1}"), new byte[0]));
    }

    @Test
    void rejectsMissingRequiredField() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_READY", "{}"), new byte[0]));
    }

    @Test
    void rejectsOmittedOptionalField() {
        String data = "{\"app_id\":\"" + APP_ID + "\",\"client_app_version\":\"" + VERSION
                + "\",\"method\":\"" + METHOD + "\",\"payload_hash\":\"" + HASH
                + "\",\"principal_id\":\"" + PRINCIPAL_ID + "\"}";
        assertThrows(IpcFormatException.class, () -> decode(envelope("IPC_CALL", data), new byte[0]));
    }

    @Test
    void rejectsNonVersion7Uuid() {
        String v4 = "01970000-0000-4000-8000-000000000001";
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_READY", "{\"app_id\":\"" + v4 + "\"}"), new byte[0]));
    }

    @Test
    void rejectsUnknownErrorCode() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_ERROR", "{\"code\":\"BOGUS\",\"message\":\"x\",\"retryable\":false,"
                        + "\"retry_after_ms\":null,\"details\":{}}"), new byte[0]));
    }

    @Test
    void rejectsLowercaseErrorCode() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_ERROR", "{\"code\":\"not_found\",\"message\":\"x\",\"retryable\":false,"
                        + "\"retry_after_ms\":null,\"details\":{}}"), new byte[0]));
    }

    @Test
    void rejectsFractionalSequence() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_EVENT_ACKED", "{\"subscription_id\":\"" + SUB_ID + "\",\"sequence\":1.5}"),
                new byte[0]));
    }

    @Test
    void rejectsNegativeWindow() {
        assertThrows(IpcFormatException.class, () -> decode(
                envelope("IPC_SUBSCRIBED", "{\"subscription_id\":\"" + SUB_ID + "\",\"app_id\":\"" + APP_ID
                        + "\",\"topic\":\"" + TOPIC + "\",\"cursor\":\"" + CURSOR + "\",\"window\":-1}"),
                new byte[0]));
    }

    @Test
    void rejectsErrorMessageLongerThan512Bytes() {
        String tooLong = "x".repeat(513);
        assertThrows(IpcFormatException.class, () -> IpcCodec.encode(
                new IpcMessage.Error(ErrorCode.INTERNAL, tooLong, false, null, Map.of()), REQUEST_ID, COMMUNITY_ID));
    }

    @Test
    void rejectsInvalidMethodOnEncode() {
        assertThrows(IpcFormatException.class, () -> IpcCodec.encode(
                new IpcMessage.Call(APP_ID, VERSION, "Notes.List", null, HASH, PRINCIPAL_ID, BODY),
                REQUEST_ID, COMMUNITY_ID));
    }

    @Test
    void rejectsUppercaseHashOnDecode() {
        String data = "{\"app_id\":\"" + APP_ID + "\",\"client_app_version\":\"" + VERSION
                + "\",\"method\":\"" + METHOD + "\",\"operation_id\":null,\"payload_hash\":\""
                + "AB".repeat(32) + "\",\"principal_id\":\"" + PRINCIPAL_ID + "\"}";
        assertThrows(IpcFormatException.class, () -> decode(envelope("IPC_CALL", data), BODY.bytes()));
    }

    private static JsonNode jsonNumber(int value) {
        return new ObjectMapper().getNodeFactory().numberNode(value);
    }
}
