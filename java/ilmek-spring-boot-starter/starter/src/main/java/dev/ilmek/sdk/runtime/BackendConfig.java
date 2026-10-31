package dev.ilmek.sdk.runtime;

import com.fasterxml.jackson.annotation.JsonProperty;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.io.InputStream;
import java.time.Duration;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

public record BackendConfig(
        @JsonProperty(value = "token", required = true) String token,
        @JsonProperty(value = "ipc_port", required = true) int ipcPort,
        @JsonProperty(value = "community_id", required = true) String communityId,
        @JsonProperty(value = "app_id", required = true) String appId,
        @JsonProperty(value = "data_dir", required = true) String dataDir) {

    private static final int MAX_BYTES = 64 * 1024;

    private static final ObjectMapper MAPPER = new ObjectMapper()
            .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);

    public static BackendConfig readFrom(InputStream in, Duration deadline) {
        byte[] bytes = readBounded(in, deadline);
        if (bytes.length == 0) {
            throw ConfigException.absent("no backend configuration on standard input");
        }
        BackendConfig config;
        try {
            config = MAPPER.readValue(bytes, BackendConfig.class);
        } catch (IOException | RuntimeException e) {
            throw new ConfigException("invalid backend configuration", e);
        }
        if (config == null) {
            throw new ConfigException("invalid backend configuration");
        }
        return config;
    }

    private static byte[] readBounded(InputStream in, Duration deadline) {
        FutureTask<byte[]> task = new FutureTask<>(() -> in.readNBytes(MAX_BYTES + 1));
        Thread.ofVirtual().name("ilmek-backend-config").start(task);
        byte[] bytes;
        try {
            bytes = task.get(deadline.toNanos(), TimeUnit.NANOSECONDS);
        } catch (TimeoutException e) {
            task.cancel(true);
            throw ConfigException.absent("backend configuration not received before the deadline");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new ConfigException("interrupted while reading backend configuration", e);
        } catch (ExecutionException e) {
            throw new ConfigException("could not read backend configuration", e.getCause());
        }
        if (bytes.length > MAX_BYTES) {
            throw new ConfigException("backend configuration exceeds " + MAX_BYTES + " bytes");
        }
        return bytes;
    }
}
