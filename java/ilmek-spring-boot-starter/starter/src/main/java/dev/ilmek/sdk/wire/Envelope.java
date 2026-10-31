package dev.ilmek.sdk.wire;

import com.fasterxml.jackson.databind.JsonNode;

public record Envelope(String type, String requestId, String communityId, JsonNode data) {
}
