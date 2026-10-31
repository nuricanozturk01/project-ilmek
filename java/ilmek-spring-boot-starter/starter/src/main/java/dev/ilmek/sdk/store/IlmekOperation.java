package dev.ilmek.sdk.store;

import jakarta.persistence.Column;
import jakarta.persistence.Embeddable;
import jakarta.persistence.EmbeddedId;
import jakarta.persistence.Entity;
import jakarta.persistence.Table;
import java.io.Serializable;
import java.util.Objects;

@Entity
@Table(name = "ilmek_operations")
public class IlmekOperation {

    @EmbeddedId
    private Key key;

    @Column(name = "method", nullable = false)
    private String method;

    @Column(name = "payload_hash", nullable = false)
    private String payloadHash;

    @Column(name = "result", nullable = false, length = 1_048_576)
    private byte[] result;

    @Column(name = "created_at_ms", nullable = false)
    private long createdAtMs;

    protected IlmekOperation() {
    }

    public IlmekOperation(String principalId, String operationId, String method, String payloadHash,
                          byte[] result, long createdAtMs) {
        this.key = new Key(principalId, operationId);
        this.method = method;
        this.payloadHash = payloadHash;
        this.result = result;
        this.createdAtMs = createdAtMs;
    }

    public Key getKey() {
        return key;
    }

    public String getMethod() {
        return method;
    }

    public String getPayloadHash() {
        return payloadHash;
    }

    public byte[] getResult() {
        return result;
    }

    public long getCreatedAtMs() {
        return createdAtMs;
    }

    @Embeddable
    public static class Key implements Serializable {

        @Column(name = "principal_id", nullable = false)
        private String principalId;

        @Column(name = "operation_id", nullable = false)
        private String operationId;

        protected Key() {
        }

        public Key(String principalId, String operationId) {
            this.principalId = principalId;
            this.operationId = operationId;
        }

        public String getPrincipalId() {
            return principalId;
        }

        public String getOperationId() {
            return operationId;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Key other
                && Objects.equals(principalId, other.principalId)
                && Objects.equals(operationId, other.operationId);
        }

        @Override
        public int hashCode() {
            return Objects.hash(principalId, operationId);
        }
    }
}
