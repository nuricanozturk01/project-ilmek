package dev.ilmek.sdk.store;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import dev.ilmek.sdk.wire.ErrorCode;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Propagation;
import org.springframework.transaction.annotation.Transactional;

@DataJpaTest
@Import(IlmekOperationsService.class)
@Transactional(propagation = Propagation.NOT_SUPPORTED)
class IlmekOperationsServiceTest {

    @Autowired
    private IlmekOperationsService service;

    @Autowired
    private IlmekOperationRepository repository;

    @AfterEach
    void clean() {
        repository.deleteAll();
    }

    @Test
    void firstWriteRunsBusinessLogicAndStoresResult() {
        AtomicInteger calls = new AtomicInteger();
        byte[] r = service.write("p1", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "ok".getBytes(UTF_8);
        });

        assertEquals("ok", new String(r, UTF_8));
        assertEquals(1, calls.get());
        IlmekOperation stored = repository.findById(new IlmekOperation.Key("p1", "op1")).orElseThrow();
        assertEquals("tasks.create", stored.getMethod());
        assertEquals("h1", stored.getPayloadHash());
        assertArrayEquals("ok".getBytes(UTF_8), stored.getResult());
    }

    @Test
    void identicalRetryReplaysWithoutRerunningLogic() {
        AtomicInteger calls = new AtomicInteger();
        Supplier<byte[]> logic = () -> {
            calls.incrementAndGet();
            return "ok".getBytes(UTF_8);
        };
        service.write("p1", "op1", "tasks.create", "h1", logic);
        byte[] r2 = service.write("p1", "op1", "tasks.create", "h1", logic);

        assertEquals("ok", new String(r2, UTF_8));
        assertEquals(1, calls.get());
    }

    @Test
    void conflictingRetryThrowsAndDoesNotRerunLogic() {
        AtomicInteger calls = new AtomicInteger();
        service.write("p1", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "ok".getBytes(UTF_8);
        });

        OperationConflictException e = assertThrows(OperationConflictException.class, () ->
            service.write("p1", "op1", "tasks.create", "h2", () -> {
                calls.incrementAndGet();
                return "bad".getBytes(UTF_8);
            }));

        assertEquals(ErrorCode.OPERATION_CONFLICT, e.code());
        assertFalse(e.retryable());
        assertEquals(1, calls.get());
        assertArrayEquals("ok".getBytes(UTF_8),
            repository.findById(new IlmekOperation.Key("p1", "op1")).orElseThrow().getResult());
    }

    @Test
    void reusedOperationIdWithDifferentMethodThrowsAndDoesNotRerunLogic() {
        AtomicInteger calls = new AtomicInteger();
        service.write("p1", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "ok".getBytes(UTF_8);
        });

        assertThrows(OperationConflictException.class, () ->
            service.write("p1", "op1", "tasks.delete", "h1", () -> {
                calls.incrementAndGet();
                return "bad".getBytes(UTF_8);
            }));

        assertEquals(1, calls.get());
    }

    @Test
    void throwingBusinessLogicRollsBackAllRowsAndRetryRunsLogicAgain() {
        AtomicInteger calls = new AtomicInteger();
        IlmekOperation.Key sideKey = new IlmekOperation.Key("p1", "side-effect");

        assertThrows(IllegalStateException.class, () ->
            service.write("p1", "op1", "tasks.create", "h1", () -> {
                calls.incrementAndGet();
                repository.saveAndFlush(new IlmekOperation("p1", "side-effect", "tasks.create", "h1",
                    "partial".getBytes(UTF_8), 1L));
                throw new IllegalStateException("business failure");
            }));

        assertFalse(repository.existsById(new IlmekOperation.Key("p1", "op1")));
        assertFalse(repository.existsById(sideKey));

        byte[] r = service.write("p1", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "ok".getBytes(UTF_8);
        });

        assertEquals("ok", new String(r, UTF_8));
        assertEquals(2, calls.get());
        assertTrue(repository.existsById(new IlmekOperation.Key("p1", "op1")));
    }

    @Test
    void differentPrincipalsWithSameOperationIdAreIndependent() {
        AtomicInteger calls = new AtomicInteger();

        byte[] r1 = service.write("p1", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "first".getBytes(UTF_8);
        });
        byte[] r2 = service.write("p2", "op1", "tasks.create", "h1", () -> {
            calls.incrementAndGet();
            return "second".getBytes(UTF_8);
        });

        assertEquals(2, calls.get());
        assertEquals("first", new String(r1, UTF_8));
        assertEquals("second", new String(r2, UTF_8));
        assertEquals("first", new String(
            repository.findById(new IlmekOperation.Key("p1", "op1")).orElseThrow().getResult(), UTF_8));
        assertEquals("second", new String(
            repository.findById(new IlmekOperation.Key("p2", "op1")).orElseThrow().getResult(), UTF_8));
    }
}
