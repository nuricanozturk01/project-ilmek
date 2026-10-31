package dev.ilmek.sdk.api;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertFalse;

import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.BeanCreationException;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

class IlmekMethodRegistryTest {

    @Configuration
    static class Fixture {
        @Bean
        TaskService svc() {
            return new TaskService();
        }

        @Bean
        IlmekMethodRegistry registry() {
            return new IlmekMethodRegistry();
        }
    }

    static class TaskService {
        @IlmekMethod(name = "tasks.list", kind = MethodKind.READ)
        byte[] list(IlmekCallContext ctx, byte[] body) {
            return "[]".getBytes(UTF_8);
        }

        @IlmekMethod(name = "tasks.add", kind = MethodKind.WRITE)
        byte[] add(IlmekCallContext ctx, byte[] body) {
            if (body.length == 0) {
                throw new AppException(ErrorCode.INVALID_MESSAGE, "empty body", false);
            }
            if (body[0] == 'x') {
                throw new IllegalArgumentException("boom");
            }
            return ctx.principalId().getBytes(UTF_8);
        }
    }

    static class DuplicateA {
        @IlmekMethod(name = "tasks.list", kind = MethodKind.READ)
        byte[] list(IlmekCallContext ctx, byte[] body) {
            return new byte[0];
        }
    }

    static class DuplicateB {
        @IlmekMethod(name = "tasks.list", kind = MethodKind.READ)
        byte[] list(IlmekCallContext ctx, byte[] body) {
            return new byte[0];
        }
    }

    @Configuration
    static class DuplicateFixture {
        @Bean
        DuplicateA a() {
            return new DuplicateA();
        }

        @Bean
        DuplicateB b() {
            return new DuplicateB();
        }

        @Bean
        IlmekMethodRegistry registry() {
            return new IlmekMethodRegistry();
        }
    }

    static class InvalidNameService {
        @IlmekMethod(name = "Tasks.List", kind = MethodKind.READ)
        byte[] list(IlmekCallContext ctx, byte[] body) {
            return new byte[0];
        }
    }

    @Configuration
    static class InvalidNameFixture {
        @Bean
        InvalidNameService svc() {
            return new InvalidNameService();
        }

        @Bean
        IlmekMethodRegistry registry() {
            return new IlmekMethodRegistry();
        }
    }

    static class WrongSignatureService {
        @IlmekMethod(name = "tasks.bad", kind = MethodKind.READ)
        String bad(IlmekCallContext ctx, byte[] body) {
            return "";
        }
    }

    @Configuration
    static class WrongSignatureFixture {
        @Bean
        WrongSignatureService svc() {
            return new WrongSignatureService();
        }

        @Bean
        IlmekMethodRegistry registry() {
            return new IlmekMethodRegistry();
        }
    }

    @Test
    void findsRegisteredMethod() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        var handle = reg.find("tasks.list");
        assertTrue(handle.isPresent());
        assertSame(MethodKind.READ, handle.get().kind());
        ctx.close();
    }

    @Test
    void returnsEmptyForUnknownMethod() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        assertFalse(reg.find("tasks.bogus").isPresent());
        ctx.close();
    }

    @Test
    void invokesAnnotatedMethodWithContextAndBody() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        var callCtx = new IlmekCallContext("app-1", "device-7", null, null);
        byte[] out = reg.find("tasks.list").orElseThrow().invoke().apply(callCtx, new byte[0]);
        assertArrayEquals("[]".getBytes(UTF_8), out);
        ctx.close();
    }

    @Test
    void writeMethodReceivesPrincipalFromContext() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        var handle = reg.find("tasks.add").orElseThrow();
        assertSame(MethodKind.WRITE, handle.kind());
        var callCtx = new IlmekCallContext("app-1", "device-7", "op-1", "hash-1");
        assertArrayEquals("device-7".getBytes(UTF_8), handle.invoke().apply(callCtx, new byte[] {1}));
        ctx.close();
    }

    @Test
    void appExceptionFromMethodPropagatesUnchanged() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        var handle = reg.find("tasks.add").orElseThrow();
        var callCtx = new IlmekCallContext("app-1", "device-7", "op-1", "hash-1");
        AppException e = assertThrows(AppException.class,
                () -> handle.invoke().apply(callCtx, new byte[0]));
        assertEquals(ErrorCode.INVALID_MESSAGE, e.code());
        assertEquals("empty body", e.getMessage());
        assertFalse(e.retryable());
        ctx.close();
    }

    @Test
    void runtimeExceptionFromMethodPropagatesUnchanged() {
        var ctx = new AnnotationConfigApplicationContext(Fixture.class);
        IlmekMethodRegistry reg = ctx.getBean(IlmekMethodRegistry.class);
        var handle = reg.find("tasks.add").orElseThrow();
        var callCtx = new IlmekCallContext("app-1", "device-7", "op-1", "hash-1");
        assertThrows(IllegalArgumentException.class,
                () -> handle.invoke().apply(callCtx, new byte[] {'x'}));
        ctx.close();
    }

    @Test
    void rejectsDuplicateMethodNameAtStartup() {
        BeanCreationException e = assertThrows(BeanCreationException.class,
                () -> new AnnotationConfigApplicationContext(DuplicateFixture.class).close());
        IllegalStateException cause = assertInstanceOf(IllegalStateException.class, e.getRootCause());
        assertTrue(cause.getMessage().contains("tasks.list"));
    }

    @Test
    void rejectsInvalidMethodNameAtStartup() {
        BeanCreationException e = assertThrows(BeanCreationException.class,
                () -> new AnnotationConfigApplicationContext(InvalidNameFixture.class).close());
        IllegalStateException cause = assertInstanceOf(IllegalStateException.class, e.getRootCause());
        assertTrue(cause.getMessage().contains("Tasks.List"));
    }

    @Test
    void rejectsUnsupportedSignatureAtStartupNamingBeanAndMethod() {
        BeanCreationException e = assertThrows(BeanCreationException.class,
                () -> new AnnotationConfigApplicationContext(WrongSignatureFixture.class).close());
        IllegalStateException cause = assertInstanceOf(IllegalStateException.class, e.getRootCause());
        assertTrue(cause.getMessage().contains("svc"));
        assertTrue(cause.getMessage().contains("bad"));
    }
}
