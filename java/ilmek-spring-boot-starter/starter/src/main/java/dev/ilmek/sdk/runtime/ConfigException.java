package dev.ilmek.sdk.runtime;

public final class ConfigException extends RuntimeException {

    private final boolean absent;

    public ConfigException(String message) {
        this(message, null, false);
    }

    public ConfigException(String message, Throwable cause) {
        this(message, cause, false);
    }

    private ConfigException(String message, Throwable cause, boolean absent) {
        super(message, cause);
        this.absent = absent;
    }

    public static ConfigException absent(String message) {
        return new ConfigException(message, null, true);
    }

    public boolean isAbsent() {
        return absent;
    }
}
