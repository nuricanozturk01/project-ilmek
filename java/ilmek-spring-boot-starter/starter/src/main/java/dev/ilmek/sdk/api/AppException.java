package dev.ilmek.sdk.api;

import dev.ilmek.sdk.wire.ErrorCode;

public class AppException extends RuntimeException {

    private final ErrorCode code;
    private final boolean retryable;

    public AppException(ErrorCode code, String message, boolean retryable) {
        super(message);
        this.code = code;
        this.retryable = retryable;
    }

    public ErrorCode code() {
        return code;
    }

    public boolean retryable() {
        return retryable;
    }
}
