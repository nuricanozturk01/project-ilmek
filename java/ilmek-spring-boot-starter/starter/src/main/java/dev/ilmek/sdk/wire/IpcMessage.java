package dev.ilmek.sdk.wire;

import com.fasterxml.jackson.databind.JsonNode;
import java.util.Arrays;
import java.util.Map;

public sealed interface IpcMessage {

    record Hello(String token, String appId) implements IpcMessage {
    }

    record Ready(String appId) implements IpcMessage {
    }

    record Call(String appId, String clientAppVersion, String method, String operationId,
                String payloadHash, String principalId, Body body) implements IpcMessage {
    }

    record Result(String appId, String operationId, Body body) implements IpcMessage {
    }

    record Error(ErrorCode code, String message, boolean retryable, Long retryAfterMs,
                 Map<String, JsonNode> details) implements IpcMessage {
    }

    record Subscribe(String appId, String clientAppVersion, String topic, String cursor,
                     String principalId) implements IpcMessage {
    }

    record Subscribed(String subscriptionId, String appId, String topic, String cursor,
                      long window) implements IpcMessage {
    }

    record Event(String subscriptionId, String appId, String topic, long sequence, String eventId,
                 String cursor, Body body) implements IpcMessage {
    }

    record EventAck(String subscriptionId, long sequence, String cursor) implements IpcMessage {
    }

    record EventAcked(String subscriptionId, long sequence) implements IpcMessage {
    }

    record Unsubscribe(String subscriptionId) implements IpcMessage {
    }

    record Unsubscribed(String subscriptionId) implements IpcMessage {
    }

    record Body(byte[] bytes) {

        @Override
        public boolean equals(Object other) {
            return other instanceof Body that && Arrays.equals(bytes, that.bytes);
        }

        @Override
        public int hashCode() {
            return Arrays.hashCode(bytes);
        }
    }
}
