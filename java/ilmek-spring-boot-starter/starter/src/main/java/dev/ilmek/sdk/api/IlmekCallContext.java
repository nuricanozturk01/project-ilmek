package dev.ilmek.sdk.api;

// principalId is supplied by the host and is not authenticated; it must not be used as a security boundary.
public record IlmekCallContext(String appId, String principalId, String operationId, String payloadHash) {
}
