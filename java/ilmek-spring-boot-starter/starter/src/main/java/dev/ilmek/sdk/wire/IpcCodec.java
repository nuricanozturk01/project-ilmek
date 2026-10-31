package dev.ilmek.sdk.wire;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import dev.ilmek.sdk.wire.IpcMessage.Body;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

public final class IpcCodec {

    public record Decoded(String requestId, String communityId, IpcMessage message) {
    }

    private static final ObjectMapper MAPPER = new ObjectMapper();
    private static final Pattern UUID_V7 =
            Pattern.compile("[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}");
    private static final Pattern METHOD = Pattern.compile("[a-z][a-z0-9_.]{0,63}");
    private static final Pattern VERSION = Pattern.compile("(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)");
    private static final Pattern HASH = Pattern.compile("[0-9a-f]{64}");
    private static final Pattern CURSOR = Pattern.compile("[A-Za-z0-9_-]{1,512}");
    private static final Pattern TOKEN = Pattern.compile("[A-Za-z0-9_-]*");
    private static final long MAX_VERSION_COMPONENT = Integer.MAX_VALUE;
    private static final int MAX_TEXT_BYTES = 512;

    private IpcCodec() {
    }

    public static byte[] encode(IpcMessage message, String requestId, String communityId) {
        ObjectNode data = MAPPER.createObjectNode();
        String type;
        byte[] body = new byte[0];
        switch (message) {
            case IpcMessage.Hello m -> {
                type = "IPC_HELLO";
                data.put("token", token("token", m.token()));
                data.put("app_id", uuid("app_id", m.appId()));
            }
            case IpcMessage.Ready m -> {
                type = "IPC_READY";
                data.put("app_id", uuid("app_id", m.appId()));
            }
            case IpcMessage.Call m -> {
                type = "IPC_CALL";
                data.put("app_id", uuid("app_id", m.appId()));
                data.put("client_app_version", version("client_app_version", m.clientAppVersion()));
                data.put("method", method("method", m.method()));
                data.put("operation_id", optionalUuid("operation_id", m.operationId()));
                data.put("payload_hash", hash("payload_hash", m.payloadHash()));
                data.put("principal_id", uuid("principal_id", m.principalId()));
                body = m.body().bytes();
            }
            case IpcMessage.Result m -> {
                type = "IPC_RESULT";
                data.put("app_id", uuid("app_id", m.appId()));
                data.put("operation_id", optionalUuid("operation_id", m.operationId()));
                body = m.body().bytes();
            }
            case IpcMessage.Error m -> {
                type = "IPC_ERROR";
                data.put("code", java.util.Objects.requireNonNull(m.code(), "code").name());
                data.put("message", text512("message", m.message()));
                data.put("retryable", m.retryable());
                data.put("retry_after_ms", optionalU64("retry_after_ms", m.retryAfterMs()));
                ObjectNode details = data.putObject("details");
                for (Map.Entry<String, JsonNode> entry : m.details().entrySet()) {
                    details.set(entry.getKey(), entry.getValue());
                }
            }
            case IpcMessage.Subscribe m -> {
                type = "IPC_SUBSCRIBE";
                data.put("app_id", uuid("app_id", m.appId()));
                data.put("client_app_version", version("client_app_version", m.clientAppVersion()));
                data.put("topic", method("topic", m.topic()));
                data.put("cursor", optionalCursor("cursor", m.cursor()));
                data.put("principal_id", uuid("principal_id", m.principalId()));
            }
            case IpcMessage.Subscribed m -> {
                type = "IPC_SUBSCRIBED";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
                data.put("app_id", uuid("app_id", m.appId()));
                data.put("topic", method("topic", m.topic()));
                data.put("cursor", cursor("cursor", m.cursor()));
                data.put("window", u64("window", m.window()));
            }
            case IpcMessage.Event m -> {
                type = "IPC_EVENT";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
                data.put("app_id", uuid("app_id", m.appId()));
                data.put("topic", method("topic", m.topic()));
                data.put("sequence", u64("sequence", m.sequence()));
                data.put("event_id", uuid("event_id", m.eventId()));
                data.put("cursor", cursor("cursor", m.cursor()));
                body = m.body().bytes();
            }
            case IpcMessage.EventAck m -> {
                type = "IPC_EVENT_ACK";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
                data.put("sequence", u64("sequence", m.sequence()));
                data.put("cursor", cursor("cursor", m.cursor()));
            }
            case IpcMessage.EventAcked m -> {
                type = "IPC_EVENT_ACKED";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
                data.put("sequence", u64("sequence", m.sequence()));
            }
            case IpcMessage.Unsubscribe m -> {
                type = "IPC_UNSUBSCRIBE";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
            }
            case IpcMessage.Unsubscribed m -> {
                type = "IPC_UNSUBSCRIBED";
                data.put("subscription_id", uuid("subscription_id", m.subscriptionId()));
            }
        }
        ObjectNode envelope = MAPPER.createObjectNode();
        envelope.put("type", type);
        envelope.put("request_id", uuid("request_id", requestId));
        envelope.put("community_id", uuid("community_id", communityId));
        envelope.set("data", data);
        byte[] metadata;
        try {
            metadata = MAPPER.writeValueAsBytes(envelope);
        } catch (JsonProcessingException e) {
            throw new IpcFormatException("IPC message could not be serialized: " + e.getOriginalMessage());
        }
        return Frame.encode(metadata, body);
    }

    public static IpcMessage decode(byte[] metadata, byte[] body) {
        return decodeWithHeader(metadata, body).message();
    }

    public static Decoded decodeWithHeader(byte[] metadata, byte[] body) {
        Envelope envelope = StrictJson.parseEnvelope(metadata);
        String requestId = uuid("request_id", envelope.requestId());
        String communityId = uuid("community_id", envelope.communityId());
        if (!(envelope.data() instanceof ObjectNode d)) {
            throw new IpcFormatException("`data` must be an object");
        }
        IpcMessage message = switch (envelope.type()) {
            case "IPC_HELLO" -> {
                bare(body);
                fields(d, "token", "app_id");
                yield new IpcMessage.Hello(token(d, "token"), uuid(d, "app_id"));
            }
            case "IPC_READY" -> {
                bare(body);
                fields(d, "app_id");
                yield new IpcMessage.Ready(uuid(d, "app_id"));
            }
            case "IPC_CALL" -> {
                fields(d, "app_id", "client_app_version", "method", "operation_id", "payload_hash", "principal_id");
                yield new IpcMessage.Call(uuid(d, "app_id"), version(d, "client_app_version"),
                        method(d, "method"), optionalUuid(d, "operation_id"), hash(d, "payload_hash"),
                        uuid(d, "principal_id"), new Body(body));
            }
            case "IPC_RESULT" -> {
                fields(d, "app_id", "operation_id");
                yield new IpcMessage.Result(uuid(d, "app_id"), optionalUuid(d, "operation_id"), new Body(body));
            }
            case "IPC_ERROR" -> {
                bare(body);
                fields(d, "code", "message", "retryable", "retry_after_ms", "details");
                yield new IpcMessage.Error(errorCode(d, "code"), text512(d, "message"), bool(d, "retryable"),
                        optionalU64(d, "retry_after_ms"), details(d, "details"));
            }
            case "IPC_SUBSCRIBE" -> {
                bare(body);
                fields(d, "app_id", "client_app_version", "topic", "cursor", "principal_id");
                yield new IpcMessage.Subscribe(uuid(d, "app_id"), version(d, "client_app_version"),
                        method(d, "topic"), optionalCursor(d, "cursor"), uuid(d, "principal_id"));
            }
            case "IPC_SUBSCRIBED" -> {
                bare(body);
                fields(d, "subscription_id", "app_id", "topic", "cursor", "window");
                yield new IpcMessage.Subscribed(uuid(d, "subscription_id"), uuid(d, "app_id"),
                        method(d, "topic"), cursor(d, "cursor"), u64(d, "window"));
            }
            case "IPC_EVENT" -> {
                fields(d, "subscription_id", "app_id", "topic", "sequence", "event_id", "cursor");
                yield new IpcMessage.Event(uuid(d, "subscription_id"), uuid(d, "app_id"), method(d, "topic"),
                        u64(d, "sequence"), uuid(d, "event_id"), cursor(d, "cursor"), new Body(body));
            }
            case "IPC_EVENT_ACK" -> {
                bare(body);
                fields(d, "subscription_id", "sequence", "cursor");
                yield new IpcMessage.EventAck(uuid(d, "subscription_id"), u64(d, "sequence"), cursor(d, "cursor"));
            }
            case "IPC_EVENT_ACKED" -> {
                bare(body);
                fields(d, "subscription_id", "sequence");
                yield new IpcMessage.EventAcked(uuid(d, "subscription_id"), u64(d, "sequence"));
            }
            case "IPC_UNSUBSCRIBE" -> {
                bare(body);
                fields(d, "subscription_id");
                yield new IpcMessage.Unsubscribe(uuid(d, "subscription_id"));
            }
            case "IPC_UNSUBSCRIBED" -> {
                bare(body);
                fields(d, "subscription_id");
                yield new IpcMessage.Unsubscribed(uuid(d, "subscription_id"));
            }
            default -> throw new IpcFormatException("unknown IPC message type `" + envelope.type() + "`");
        };
        return new Decoded(requestId, communityId, message);
    }

    private static void bare(byte[] body) {
        if (body.length != 0) {
            throw new IpcFormatException("message must not carry a body");
        }
    }

    private static void fields(ObjectNode d, String... names) {
        Set<String> expected = Set.of(names);
        if (expected.size() != names.length) {
            throw new IllegalStateException("duplicate field name in codec table");
        }
        Set<String> actual = new java.util.HashSet<>();
        d.fieldNames().forEachRemaining(actual::add);
        if (!actual.equals(expected)) {
            throw new IpcFormatException("data fields must be exactly " + expected + " but were " + actual);
        }
    }

    private static JsonNode node(ObjectNode d, String name) {
        JsonNode value = d.get(name);
        if (value == null) {
            throw new IpcFormatException("missing field `" + name + "`");
        }
        return value;
    }

    private static String text(ObjectNode d, String name) {
        JsonNode value = node(d, name);
        if (!value.isTextual()) {
            throw new IpcFormatException("`" + name + "` must be a string");
        }
        return value.textValue();
    }

    private static String optionalText(ObjectNode d, String name) {
        return node(d, name).isNull() ? null : text(d, name);
    }

    private static String uuid(ObjectNode d, String name) {
        return uuid(name, text(d, name));
    }

    private static String optionalUuid(ObjectNode d, String name) {
        String value = optionalText(d, name);
        return value == null ? null : uuid(name, value);
    }

    private static String method(ObjectNode d, String name) {
        return method(name, text(d, name));
    }

    private static String version(ObjectNode d, String name) {
        return version(name, text(d, name));
    }

    private static String hash(ObjectNode d, String name) {
        return hash(name, text(d, name));
    }

    private static String cursor(ObjectNode d, String name) {
        return cursor(name, text(d, name));
    }

    private static String optionalCursor(ObjectNode d, String name) {
        String value = optionalText(d, name);
        return value == null ? null : cursor(name, value);
    }

    private static String token(ObjectNode d, String name) {
        return token(name, text(d, name));
    }

    private static String text512(ObjectNode d, String name) {
        return text512(name, text(d, name));
    }

    private static boolean bool(ObjectNode d, String name) {
        JsonNode value = node(d, name);
        if (!value.isBoolean()) {
            throw new IpcFormatException("`" + name + "` must be a boolean");
        }
        return value.booleanValue();
    }

    private static long u64(ObjectNode d, String name) {
        JsonNode value = node(d, name);
        if (!value.isIntegralNumber() || !value.canConvertToLong() || value.longValue() < 0) {
            throw new IpcFormatException("`" + name + "` must be an unsigned integer");
        }
        return value.longValue();
    }

    private static Long optionalU64(ObjectNode d, String name) {
        return node(d, name).isNull() ? null : u64(d, name);
    }

    private static ErrorCode errorCode(ObjectNode d, String name) {
        String value = text(d, name);
        try {
            return ErrorCode.valueOf(value);
        } catch (IllegalArgumentException e) {
            throw new IpcFormatException("unknown error code `" + value + "`");
        }
    }

    private static Map<String, JsonNode> details(ObjectNode d, String name) {
        JsonNode value = node(d, name);
        if (!value.isObject()) {
            throw new IpcFormatException("`" + name + "` must be an object");
        }
        Map<String, JsonNode> out = new LinkedHashMap<>();
        value.properties().forEach(entry -> out.put(entry.getKey(), entry.getValue()));
        return out;
    }

    private static String uuid(String name, String value) {
        if (value == null || !UUID_V7.matcher(value).matches()) {
            throw new IpcFormatException("`" + name + "` must be a canonical UUIDv7");
        }
        return value;
    }

    private static String optionalUuid(String name, String value) {
        return value == null ? null : uuid(name, value);
    }

    private static String method(String name, String value) {
        if (value == null || !METHOD.matcher(value).matches()) {
            throw new IpcFormatException("`" + name + "` must match [a-z][a-z0-9_.]{0,63}");
        }
        return value;
    }

    private static String version(String name, String value) {
        if (value == null || !VERSION.matcher(value).matches()) {
            throw new IpcFormatException("`" + name + "` must be major.minor.patch");
        }
        for (String part : value.split("\\.")) {
            if (part.length() > 10 || Long.parseLong(part) > MAX_VERSION_COMPONENT) {
                throw new IpcFormatException("`" + name + "` component exceeds 2^31-1");
            }
        }
        return value;
    }

    private static String hash(String name, String value) {
        if (value == null || !HASH.matcher(value).matches()) {
            throw new IpcFormatException("`" + name + "` must be 64 lowercase hex characters");
        }
        return value;
    }

    private static String cursor(String name, String value) {
        if (value == null || !CURSOR.matcher(value).matches()) {
            throw new IpcFormatException("`" + name + "` must be 1..512 base64url characters");
        }
        return value;
    }

    private static String optionalCursor(String name, String value) {
        return value == null ? null : cursor(name, value);
    }

    private static String token(String name, String value) {
        if (value == null || !TOKEN.matcher(value).matches() || value.length() % 4 == 1) {
            throw new IpcFormatException("`" + name + "` must be unpadded base64url");
        }
        return value;
    }

    private static String text512(String name, String value) {
        if (value == null || value.getBytes(StandardCharsets.UTF_8).length > MAX_TEXT_BYTES) {
            throw new IpcFormatException("`" + name + "` must be at most 512 bytes");
        }
        return value;
    }

    private static long u64(String name, long value) {
        if (value < 0) {
            throw new IpcFormatException("`" + name + "` must be an unsigned integer");
        }
        return value;
    }

    private static Long optionalU64(String name, Long value) {
        return value == null ? null : u64(name, value);
    }
}
