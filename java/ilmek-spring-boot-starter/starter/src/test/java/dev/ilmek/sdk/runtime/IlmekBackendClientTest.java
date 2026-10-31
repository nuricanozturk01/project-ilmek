package dev.ilmek.sdk.runtime;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethod;
import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.store.EventCursor;
import dev.ilmek.sdk.store.IlmekEvent;
import dev.ilmek.sdk.store.IlmekEventFloor;
import dev.ilmek.sdk.store.IlmekEventFloorRepository;
import dev.ilmek.sdk.store.IlmekEventRepository;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import dev.ilmek.sdk.store.IlmekOperationsService;
import dev.ilmek.sdk.store.IlmekStoreTestConfiguration;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.Frame;
import dev.ilmek.sdk.wire.FrameDecoder;
import dev.ilmek.sdk.wire.IpcCodec;
import dev.ilmek.sdk.wire.IpcMessage;
import dev.ilmek.sdk.wire.MethodKind;
import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.context.annotation.Import;
import org.springframework.dao.DataIntegrityViolationException;
import org.springframework.test.context.ContextConfiguration;
import org.springframework.transaction.annotation.Propagation;
import org.springframework.transaction.annotation.Transactional;

@DataJpaTest
@ContextConfiguration(classes = IlmekStoreTestConfiguration.class)
@Import({IlmekOperationsService.class, IlmekEventsService.class})
@Transactional(propagation = Propagation.NOT_SUPPORTED)
class IlmekBackendClientTest {

    private static final String COMMUNITY = id(1);
    private static final String APP = id(2);
    private static final String PRINCIPAL = id(3);
    private static final String TOKEN = "tok";
    private static final String HASH = "a".repeat(64);
    private static final String OTHER_APP = id(4);
    private static final Duration HELLO_DEADLINE = Duration.ofSeconds(5);
    private static final String TOPIC = "notes.changed";

    @Autowired
    private IlmekOperationsService operations;

    @Autowired
    private IlmekEventsService events;

    @Autowired
    private IlmekOperationRepository operationRepository;

    @Autowired
    private IlmekEventRepository eventRepository;

    @Autowired
    private IlmekEventFloorRepository floorRepository;

    @AfterEach
    void clean() {
        operationRepository.deleteAll();
        eventRepository.deleteAll();
        floorRepository.deleteAll();
    }

    @Test
    void performsHelloReadyHandshake() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), new IlmekMethodRegistry(), failure);
            host.accept();

            IpcCodec.Decoded hello = host.read();
            IpcMessage.Hello message = assertInstanceOf(IpcMessage.Hello.class, hello.message());
            assertEquals(TOKEN, message.token());
            assertEquals(APP, message.appId());
            assertEquals(COMMUNITY, hello.communityId());

            host.send(new IpcMessage.Ready(APP), hello.requestId());
            host.disconnect();

            client.join(5_000);
            assertFalse(client.isAlive());
            assertNull(failure.get());
        }
    }

    @Test
    void readyForAnotherAppFailsTheHandshake() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), new IlmekMethodRegistry(), failure);
            host.accept();

            IpcCodec.Decoded hello = host.read();
            host.send(new IpcMessage.Ready(OTHER_APP), hello.requestId());

            client.join(5_000);
            assertFalse(client.isAlive());
            assertInstanceOf(IOException.class, failure.get());
        }
    }

    @Test
    void dispatchesIpcCallToRegisteredReadMethodAndRepliesWithResult() throws Exception {
        Methods methods = new Methods(events);
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(methods), failure);
            host.handshake();

            String request = id(100);
            host.send(call("notes.echo", null, "{\"q\":1}"), request);

            IpcCodec.Decoded reply = host.read();
            assertEquals(request, reply.requestId());
            IpcMessage.Result result = assertInstanceOf(IpcMessage.Result.class, reply.message());
            assertEquals(APP, result.appId());
            assertNull(result.operationId());
            assertArrayEquals("{\"q\":1}".getBytes(UTF_8), result.body().bytes());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void dispatchesWriteCallThroughOperationsService() throws Exception {
        Methods methods = new Methods(events);
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(methods), failure);
            host.handshake();

            String operation = id(200);
            host.send(call("notes.count", operation, "{}"), id(201));
            IpcCodec.Decoded first = host.read();
            host.send(call("notes.count", operation, "{}"), id(202));
            IpcCodec.Decoded second = host.read();

            assertEquals(id(201), first.requestId());
            assertEquals(id(202), second.requestId());
            IpcMessage.Result a = assertInstanceOf(IpcMessage.Result.class, first.message());
            IpcMessage.Result b = assertInstanceOf(IpcMessage.Result.class, second.message());
            assertEquals(operation, a.operationId());
            assertEquals(operation, b.operationId());
            assertEquals("{\"count\":1}", new String(a.body().bytes(), UTF_8));
            assertArrayEquals(a.body().bytes(), b.body().bytes());
            assertEquals(1, methods.writes.get());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void unknownMethodRepliesWithNotFoundError() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(300);
            host.send(call("notes.missing", null, "{}"), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.NOT_FOUND, error.code());
            assertFalse(error.retryable());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void readMethodCarryingOperationIdRepliesInvalidMessage() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(310);
            host.send(call("notes.echo", id(311), "{}"), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.INVALID_MESSAGE, error.code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void writeMethodWithoutOperationIdRepliesInvalidMessage() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(320);
            host.send(call("notes.count", null, "{}"), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.INVALID_MESSAGE, error.code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void nonObjectReadResultRepliesWithInternalError() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(330);
            host.send(call("notes.list", null, "{}"), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.INTERNAL, error.code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void emptyObjectReadResultRepliesWithResult() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(340);
            host.send(call("notes.empty", null, "{}"), request);

            IpcCodec.Decoded reply = host.read();
            assertEquals(request, reply.requestId());
            IpcMessage.Result result = assertInstanceOf(IpcMessage.Result.class, reply.message());
            assertArrayEquals("{}".getBytes(UTF_8), result.body().bytes());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void backToBackDuplicateWritesRunTheBusinessSideEffectOnce() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String operation = id(420);
            host.send(call("notes.record", operation, "{\"n\":1}"), id(421));
            host.send(call("notes.record", operation, "{\"n\":1}"), id(422));

            IpcCodec.Decoded first = host.read();
            IpcCodec.Decoded second = host.read();
            assertEquals(Set.of(id(421), id(422)), Set.of(first.requestId(), second.requestId()));
            for (IpcCodec.Decoded reply : List.of(first, second)) {
                if (reply.message() instanceof IpcMessage.Result result) {
                    assertEquals(operation, result.operationId());
                    assertEquals("{\"recorded\":true}", text(result.body().bytes()));
                } else {
                    IpcMessage.Error error = assertInstanceOf(IpcMessage.Error.class, reply.message());
                    assertEquals(ErrorCode.OPERATION_PENDING, error.code());
                    assertTrue(error.retryable());
                }
            }
            assertEquals(1, events.since(TOPIC, null).size());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void conflictingRetryRepliesWithOperationConflict() throws Exception {
        Methods methods = new Methods(events);
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(methods), failure);
            host.handshake();

            String operation = id(360);
            host.send(call("notes.count", operation, "{}"), id(361));
            host.read();
            host.send(new IpcMessage.Call(APP, "1.0.0", "notes.count", operation, "b".repeat(64),
                    PRINCIPAL, new IpcMessage.Body("{}".getBytes(UTF_8))), id(362));

            IpcMessage.Error error = host.readError(id(362));
            assertEquals(ErrorCode.OPERATION_CONFLICT, error.code());
            assertFalse(error.retryable());
            assertEquals(1, methods.writes.get());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void businessExceptionKeepsItsCodeAndLoopContinues() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            host.send(call("notes.forbidden", null, "{}"), id(370));
            IpcMessage.Error forbidden = host.readError(id(370));
            assertEquals(ErrorCode.FORBIDDEN, forbidden.code());
            assertEquals("not your note", forbidden.message());

            host.send(call("notes.boom", null, "{}"), id(371));
            IpcMessage.Error internal = host.readError(id(371));
            assertEquals(ErrorCode.INTERNAL, internal.code());
            assertEquals("method failed", internal.message());

            host.send(call("notes.echo", null, "{\"after\":true}"), id(372));
            IpcCodec.Decoded after = host.read();
            assertEquals(id(372), after.requestId());
            assertInstanceOf(IpcMessage.Result.class, after.message());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void expiredCursorRepliesWithCursorExpired() throws Exception {
        events.append(TOPIC, "{\"n\":1}".getBytes(UTF_8));
        floorRepository.save(new IlmekEventFloor(TOPIC, 5));
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(380);
            host.send(subscribe(TOPIC, EventCursor.encode(TOPIC, 2)), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.CURSOR_EXPIRED, error.code());
            assertFalse(error.retryable());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void crossTopicCursorRepliesWithInvalidMessage() throws Exception {
        events.append(TOPIC, "{\"n\":1}".getBytes(UTF_8));
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(390);
            host.send(subscribe(TOPIC, EventCursor.encode("other.topic", 0)), request);

            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.INVALID_MESSAGE, error.code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void subscribeFromCursorSendsRetainedEventsAfterIt() throws Exception {
        events.append(TOPIC, obj(1));
        events.append(TOPIC, obj(2));
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(400);
            host.send(subscribe(TOPIC, EventCursor.encode(TOPIC, 0)), request);

            IpcCodec.Decoded subscribed = host.read();
            assertEquals(request, subscribed.requestId());
            IpcMessage.Subscribed header = assertInstanceOf(IpcMessage.Subscribed.class, subscribed.message());
            assertEquals(TOPIC, header.topic());
            assertEquals(16L, header.window());
            assertEquals(EventCursor.encode(TOPIC, 0), header.cursor());
            assertEvent(host.read(), request, header.subscriptionId(), 1);
            assertEvent(host.read(), request, header.subscriptionId(), 2);
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void nonObjectEventBodyEndsSubscriptionWithInternalError() throws Exception {
        events.append(TOPIC, "[]".getBytes(UTF_8));
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(410);
            host.send(subscribe(TOPIC, EventCursor.encode(TOPIC, 0)), request);

            IpcCodec.Decoded subscribed = host.read();
            assertInstanceOf(IpcMessage.Subscribed.class, subscribed.message());
            IpcMessage.Error error = host.readError(request);
            assertEquals(ErrorCode.INTERNAL, error.code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void boundaryCursorWindowsEventsUntilAcknowledged() throws Exception {
        events.append(TOPIC, obj(1));
        events.append(TOPIC, obj(2));
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(430);
            host.send(subscribe(TOPIC, null), request);
            IpcCodec.Decoded subscribed = host.read();
            IpcMessage.Subscribed header = assertInstanceOf(IpcMessage.Subscribed.class, subscribed.message());
            String subscription = header.subscriptionId();
            assertEquals(16L, header.window());
            assertEquals(EventCursor.encode(TOPIC, 2), header.cursor());

            for (int n = 3; n <= 20; n++) {
                events.append(TOPIC, obj(n));
            }
            for (long sequence = 3; sequence <= 18; sequence++) {
                assertEvent(host.read(), request, subscription, sequence);
            }
            assertNull(host.tryRead(Duration.ofMillis(600)));

            host.send(new IpcMessage.EventAck(subscription, 3, EventCursor.encode(TOPIC, 3)), id(431));
            IpcCodec.Decoded acked = host.read();
            assertEquals(id(431), acked.requestId());
            assertEquals(new IpcMessage.EventAcked(subscription, 3), acked.message());
            assertEvent(host.read(), request, subscription, 19);
            assertNull(host.tryRead(Duration.ofMillis(300)));

            host.send(new IpcMessage.EventAck(subscription, 19, EventCursor.encode(TOPIC, 21)), id(432));
            assertEquals(ErrorCode.INVALID_MESSAGE, host.readError(id(432)).code());

            host.send(new IpcMessage.EventAck(subscription, 19, EventCursor.encode(TOPIC, 19)), id(433));
            assertEquals(new IpcMessage.EventAcked(subscription, 19), host.read().message());
            assertEvent(host.read(), request, subscription, 20);
            assertNull(host.tryRead(Duration.ofMillis(300)));

            host.send(new IpcMessage.EventAck(id(999), 1, EventCursor.encode(TOPIC, 1)), id(434));
            assertEquals(ErrorCode.NOT_FOUND, host.readError(id(434)).code());
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void unsubscribeStopsDeliveryAndAcknowledges() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            String request = id(440);
            host.send(subscribe(TOPIC, null), request);
            String subscription = assertInstanceOf(IpcMessage.Subscribed.class, host.read().message())
                    .subscriptionId();

            host.send(new IpcMessage.Unsubscribe(subscription), id(441));
            IpcCodec.Decoded unsubscribed = host.read();
            assertEquals(id(441), unsubscribed.requestId());
            assertEquals(new IpcMessage.Unsubscribed(subscription), unsubscribed.message());

            events.append(TOPIC, obj(1));
            assertNull(host.tryRead(Duration.ofMillis(600)));
            host.disconnect();
            client.join(5_000);
        }
    }

    @Test
    void malformedFrameEndsTheServingLoop() throws Exception {
        try (FakeHost host = new FakeHost()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread client = startClient(host.port(), registryOf(new Methods(events)), failure);
            host.handshake();

            host.sendRaw("GARBAGE-NOT-A-FRAME".getBytes(UTF_8));

            client.join(5_000);
            assertFalse(client.isAlive());
            assertNull(failure.get());
        }
    }

    private static String text(byte[] bytes) {
        return new String(bytes, UTF_8);
    }

    private static byte[] obj(int n) {
        return bytes("{\"n\":" + n + "}");
    }

    private static void assertEvent(IpcCodec.Decoded frame, String request, String subscription, long sequence) {
        assertEquals(request, frame.requestId());
        IpcMessage.Event event = assertInstanceOf(IpcMessage.Event.class, frame.message());
        assertEquals(subscription, event.subscriptionId());
        assertEquals(sequence, event.sequence());
        assertEquals(EventCursor.encode(TOPIC, sequence), event.cursor());
        assertEquals(TOPIC, event.topic());
    }

    private static String id(long n) {
        return String.format("0190f5a0-0000-7000-8000-%012d", n);
    }

    private static byte[] bytes(String text) {
        return text.getBytes(UTF_8);
    }

    private static IlmekMethodRegistry registryOf(Object bean) {
        IlmekMethodRegistry registry = new IlmekMethodRegistry();
        registry.postProcessAfterInitialization(bean, "bean");
        return registry;
    }

    private IlmekBackendClient newClient(int port, IlmekMethodRegistry registry) {
        return new IlmekBackendClient("127.0.0.1", port, TOKEN, APP, COMMUNITY, registry, operations, events);
    }

    private Thread startClient(int port, IlmekMethodRegistry registry, AtomicReference<Throwable> failure) {
        IlmekBackendClient client = newClient(port, registry);
        Thread thread = new Thread(() -> {
            try {
                client.connectAndServe(HELLO_DEADLINE);
            } catch (Throwable t) {
                failure.set(t);
            }
        }, "backend-client-test");
        thread.start();
        return thread;
    }

    private static IpcMessage.Call call(String method, String operationId, String body) {
        return new IpcMessage.Call(APP, "1.0.0", method, operationId, HASH, PRINCIPAL,
                new IpcMessage.Body(bytes(body)));
    }

    private static IpcMessage.Subscribe subscribe(String topic, String cursor) {
        return new IpcMessage.Subscribe(APP, "1.0.0", topic, cursor, PRINCIPAL);
    }

    static final class Methods {
        final AtomicInteger writes = new AtomicInteger();
        private final IlmekEventsService events;

        Methods(IlmekEventsService events) {
            this.events = events;
        }

        @IlmekMethod(name = "notes.record", kind = MethodKind.WRITE)
        byte[] record(IlmekCallContext context, byte[] body) {
            events.append(TOPIC, body);
            return bytes("{\"recorded\":true}");
        }

        @IlmekMethod(name = "notes.echo", kind = MethodKind.READ)
        byte[] echo(IlmekCallContext context, byte[] body) {
            return body;
        }

        @IlmekMethod(name = "notes.list", kind = MethodKind.READ)
        byte[] list(IlmekCallContext context, byte[] body) {
            return bytes("[]");
        }

        @IlmekMethod(name = "notes.empty", kind = MethodKind.READ)
        byte[] empty(IlmekCallContext context, byte[] body) {
            return bytes("{}");
        }

        @IlmekMethod(name = "notes.boom", kind = MethodKind.READ)
        byte[] boom(IlmekCallContext context, byte[] body) {
            throw new IllegalStateException("internal detail that must not leak");
        }

        @IlmekMethod(name = "notes.forbidden", kind = MethodKind.READ)
        byte[] forbidden(IlmekCallContext context, byte[] body) {
            throw new AppException(ErrorCode.FORBIDDEN, "not your note", false);
        }

        @IlmekMethod(name = "notes.count", kind = MethodKind.WRITE)
        byte[] count(IlmekCallContext context, byte[] body) {
            return bytes("{\"count\":" + writes.incrementAndGet() + "}");
        }

        @IlmekMethod(name = "notes.race", kind = MethodKind.WRITE)
        byte[] race(IlmekCallContext context, byte[] body) {
            throw new DataIntegrityViolationException("duplicate key");
        }
    }

    private static final class FakeHost implements AutoCloseable {

        private final ServerSocket server;
        private final FrameDecoder decoder = new FrameDecoder();
        private Socket socket;
        private InputStream in;
        private OutputStream out;

        FakeHost() throws IOException {
            server = new ServerSocket(0, 0, InetAddress.getLoopbackAddress());
        }

        int port() {
            return server.getLocalPort();
        }

        void accept() throws IOException {
            socket = server.accept();
            socket.setSoTimeout(5_000);
            in = socket.getInputStream();
            out = socket.getOutputStream();
        }

        void handshake() throws IOException {
            accept();
            IpcCodec.Decoded hello = read();
            assertInstanceOf(IpcMessage.Hello.class, hello.message());
            send(new IpcMessage.Ready(APP), hello.requestId());
        }

        IpcCodec.Decoded read() throws IOException {
            Frame frame = nextFrame();
            return IpcCodec.decodeWithHeader(frame.metadata(), frame.body());
        }

        IpcMessage.Error readError(String requestId) throws IOException {
            IpcCodec.Decoded reply = read();
            assertEquals(requestId, reply.requestId());
            return assertInstanceOf(IpcMessage.Error.class, reply.message());
        }

        void send(IpcMessage message, String requestId) throws IOException {
            out.write(IpcCodec.encode(message, requestId, COMMUNITY));
            out.flush();
        }

        void sendRaw(byte[] raw) throws IOException {
            out.write(raw);
            out.flush();
        }

        IpcCodec.Decoded tryRead(Duration wait) throws IOException {
            socket.setSoTimeout((int) wait.toMillis());
            try {
                return read();
            } catch (SocketTimeoutException e) {
                return null;
            } finally {
                socket.setSoTimeout(5_000);
            }
        }

        void disconnect() throws IOException {
            if (socket != null) {
                socket.close();
            }
        }

        @Override
        public void close() throws IOException {
            if (socket != null) {
                socket.close();
            }
            server.close();
        }

        private Frame nextFrame() throws IOException {
            byte[] buffer = new byte[8192];
            while (true) {
                Optional<Frame> frame = decoder.nextFrame();
                if (frame.isPresent()) {
                    return frame.get();
                }
                int n = in.read(buffer);
                if (n < 0) {
                    throw new EOFException("client closed the connection");
                }
                decoder.push(Arrays.copyOf(buffer, n));
            }
        }
    }
}
