package dev.ilmek.sdk.wire;

public class StrictJsonException extends RuntimeException {

    public StrictJsonException(String message) {
        super(message);
    }

    public StrictJsonException(String message, Throwable cause) {
        super(message, cause);
    }
}
