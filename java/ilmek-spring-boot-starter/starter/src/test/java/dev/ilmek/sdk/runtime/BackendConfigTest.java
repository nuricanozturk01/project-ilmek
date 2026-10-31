package dev.ilmek.sdk.runtime;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.io.ByteArrayInputStream;
import java.io.InputStream;
import java.time.Duration;
import org.junit.jupiter.api.Test;

class BackendConfigTest {

    private static final String VALID =
            "{\"token\":\"abc\",\"ipc_port\":41234,\"community_id\":\"c1\",\"app_id\":\"a1\",\"data_dir\":\"/tmp/x\"}";

    private static BackendConfig read(String json) {
        return BackendConfig.readFrom(new ByteArrayInputStream(json.getBytes(UTF_8)), Duration.ofSeconds(1));
    }

    @Test
    void parsesValidConfig() {
        BackendConfig cfg = read(VALID);
        assertEquals("abc", cfg.token());
        assertEquals(41234, cfg.ipcPort());
        assertEquals("c1", cfg.communityId());
        assertEquals("a1", cfg.appId());
        assertEquals("/tmp/x", cfg.dataDir());
    }

    @Test
    void rejectsUnknownField() {
        String json = VALID.replace("}", ",\"extra\":1}");
        assertThrows(ConfigException.class, () -> read(json));
    }

    @Test
    void rejectsMalformedJson() {
        assertThrows(ConfigException.class, () -> read("{\"token\":"));
    }

    @Test
    void rejectsMissingField() {
        String json = "{\"token\":\"abc\",\"ipc_port\":41234,\"community_id\":\"c1\",\"app_id\":\"a1\"}";
        assertThrows(ConfigException.class, () -> read(json));
    }

    @Test
    void rejectsNonObjectTopLevel() {
        assertThrows(ConfigException.class, () -> read("[1,2]"));
    }

    @Test
    void timesOutIfStdinNeverCloses() {
        InputStream neverEnds = new InputStream() {
            @Override
            public int read() {
                try {
                    Thread.sleep(500);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new IllegalStateException(e);
                }
                return 0;
            }
        };
        assertThrows(ConfigException.class, () -> BackendConfig.readFrom(neverEnds, Duration.ofMillis(50)));
    }
}
