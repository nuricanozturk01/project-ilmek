package dev.ilmek.sdk.runtime;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.store.EventCursor;
import dev.ilmek.sdk.store.IlmekEvent;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.store.IlmekOperationsService;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.Frame;
import dev.ilmek.sdk.wire.FrameDecoder;
import dev.ilmek.sdk.wire.FrameFormatException;
import dev.ilmek.sdk.wire.IpcCodec;
import dev.ilmek.sdk.wire.IpcFormatException;
import dev.ilmek.sdk.wire.IpcMessage;
import dev.ilmek.sdk.wire.MethodKind;
import dev.ilmek.sdk.wire.StrictJsonException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.UnknownHostException;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Semaphore;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import org.springframework.dao.DataIntegrityViolationException;

public final class IlmekBackendClient {

    private static final long NO_DEADLINE = Long.MAX_VALUE;
    private static final int READ_BUFFER_BYTES = 8_192;
    private static final int MAX_IN_FLIGHT = 64;
    private static final int OUTBOUND_CAPACITY = 256;
    private static final int EVENT_WINDOW = 16;
    private static final int MAX_BODY_BYTES = 262_144;
    private static final int MAX_MESSAGE_CHARS = 170;
    private static final byte[] WRITER_STOP = new byte[0];
    private static final ObjectMapper JSON = new ObjectMapper()
            .enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS, DeserializationFeature.FAIL_ON_READING_DUP_TREE_KEY);

    private final InetSocketAddress address;
    private final String token;
    private final String appId;
    private final String communityId;
    private final Map<String, IlmekMethodRegistry.MethodHandle> methods;
    private final IlmekOperationsService operations;
    private final IlmekEventsService events;
    private final BlockingQueue<byte[]> outbound = new ArrayBlockingQueue<>(OUTBOUND_CAPACITY);
    private final Semaphore inFlight = new Semaphore(MAX_IN_FLIGHT);
    private final AtomicBoolean started = new AtomicBoolean();
    private final AtomicBoolean closed = new AtomicBoolean();

    public IlmekBackendClient(String host, int port, String token, String appId, String communityId,
                              IlmekMethodRegistry registry, IlmekOperationsService operations,
                              IlmekEventsService events) {
        InetAddress local;
        try {
            local = InetAddress.getByName(host);
        } catch (UnknownHostException e) {
            throw new IllegalArgumentException("unknown backend IPC host", e);
        }
        if (!local.isLoopbackAddress()) {
            throw new IllegalArgumentException("backend IPC must connect to a loopback address");
        }
        if (port < 1 || port > 65_535) {
            throw new IllegalArgumentException("port outside 1..65535");
        }
        this.address = new InetSocketAddress(local, port);
        this.token = Objects.requireNonNull(token, "token");
        this.appId = Objects.requireNonNull(appId, "appId");
        this.communityId = Objects.requireNonNull(communityId, "communityId");
        this.methods = Objects.requireNonNull(registry, "registry").snapshot();
        this.operations = Objects.requireNonNull(operations, "operations");
        this.events = Objects.requireNonNull(events, "events");
    }

    public void connectAndServe(Duration helloDeadline) throws IOException {
        Objects.requireNonNull(helloDeadline, "helloDeadline");
        if (helloDeadline.isNegative() || helloDeadline.isZero()) {
            throw new IllegalArgumentException("helloDeadline must be positive");
        }
        if (!started.compareAndSet(false, true)) {
            throw new IllegalStateException("backend client is single-use");
        }
        byte[] hello;
        try {
            hello = IpcCodec.encode(new IpcMessage.Hello(token, appId), newUuidV7(), communityId);
        } catch (IpcFormatException e) {
            throw new IllegalArgumentException("invalid backend identity: " + e.getMessage(), e);
        }
        Socket socket = new Socket();
        try {
            socket.connect(address, (int) Math.min(Integer.MAX_VALUE, helloDeadline.toMillis()));
            socket.setTcpNoDelay(true);
            startWriter(socket);
            enqueue(hello);
            InputStream in = socket.getInputStream();
            FrameDecoder decoder = new FrameDecoder();
            byte[] buffer = new byte[READ_BUFFER_BYTES];
            handshake(socket, in, decoder, buffer, System.nanoTime() + helloDeadline.toNanos());
            serve(socket, in, decoder, buffer);
        } finally {
            closed.set(true);
            outbound.offer(WRITER_STOP);
            closeQuietly(socket);
        }
    }

    private void startWriter(Socket socket) throws IOException {
        OutputStream out = socket.getOutputStream();
        Thread.ofPlatform().name("ilmek-ipc-writer").daemon(true).start(() -> writeLoop(out, socket));
    }

    private void writeLoop(OutputStream out, Socket socket) {
        try {
            while (true) {
                byte[] frame = outbound.take();
                if (frame == WRITER_STOP) {
                    return;
                }
                out.write(frame);
                out.flush();
            }
        } catch (IOException | InterruptedException e) {
            closed.set(true);
            closeQuietly(socket);
        }
    }

    private void handshake(Socket socket, InputStream in, FrameDecoder decoder, byte[] buffer, long deadlineNanos)
            throws IOException {
        Frame frame;
        try {
            frame = readFrame(socket, in, decoder, buffer, deadlineNanos);
        } catch (FrameFormatException e) {
            throw new IOException("invalid frame during handshake", e);
        }
        if (frame == null) {
            throw new IOException("host closed the connection before IPC_READY");
        }
        IpcCodec.Decoded ready;
        try {
            ready = IpcCodec.decodeWithHeader(frame.metadata(), frame.body());
        } catch (IpcFormatException | StrictJsonException e) {
            throw new IOException("invalid message during handshake", e);
        }
        if (!(ready.message() instanceof IpcMessage.Ready r) || !r.appId().equals(appId)
                || !communityId.equals(ready.communityId())) {
            throw new IOException("expected IPC_READY for this application and community");
        }
        socket.setSoTimeout(0);
    }

    private void serve(Socket socket, InputStream in, FrameDecoder decoder, byte[] buffer) throws IOException {
        ExecutorService executor = Executors.newVirtualThreadPerTaskExecutor();
        try {
            while (true) {
                Frame frame = readFrame(socket, in, decoder, buffer, NO_DEADLINE);
                if (frame == null) {
                    return;
                }
                IpcCodec.Decoded decoded;
                try {
                    decoded = IpcCodec.decodeWithHeader(frame.metadata(), frame.body());
                } catch (IpcFormatException | StrictJsonException e) {
                    return;
                }
                switch (decoded.message()) {
                    case IpcMessage.Call call -> dispatch(executor, decoded.requestId(), decoded.communityId(),
                            () -> respond(decoded.requestId(), answerCall(call)));
                    case IpcMessage.Subscribe subscribe -> dispatch(executor, decoded.requestId(),
                            decoded.communityId(), () -> answerSubscribe(decoded.requestId(), subscribe));
                    case IpcMessage.EventAck ack ->
                            respond(decoded.requestId(), error(ErrorCode.NOT_FOUND, "unknown subscription", false));
                    case IpcMessage.Unsubscribe unsubscribe ->
                            respond(decoded.requestId(), new IpcMessage.Unsubscribed(unsubscribe.subscriptionId()));
                    default -> {
                        return;
                    }
                }
            }
        } catch (FrameFormatException e) {
            return;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } finally {
            closed.set(true);
            executor.shutdownNow();
        }
    }

    private void dispatch(ExecutorService executor, String requestId, String requestCommunity, Runnable task)
            throws InterruptedException {
        if (!communityId.equals(requestCommunity)) {
            respond(requestId, error(ErrorCode.FORBIDDEN, "community is not served by this backend", false));
            return;
        }
        inFlight.acquire();
        executor.execute(() -> {
            try {
                task.run();
            } finally {
                inFlight.release();
            }
        });
    }

    private IpcMessage answerCall(IpcMessage.Call call) {
        if (!appId.equals(call.appId())) {
            return error(ErrorCode.FORBIDDEN, "application is not served by this backend", false);
        }
        IlmekMethodRegistry.MethodHandle handle = methods.get(call.method());
        if (handle == null) {
            return error(ErrorCode.NOT_FOUND, "no such method", false);
        }
        boolean write = handle.kind() == MethodKind.WRITE;
        if (write && call.operationId() == null) {
            return error(ErrorCode.INVALID_MESSAGE, "writes need an operation id", false);
        }
        if (!write && call.operationId() != null) {
            return error(ErrorCode.INVALID_MESSAGE, "reads must not carry an operation id", false);
        }
        IlmekCallContext context = new IlmekCallContext(appId, call.principalId(), call.operationId(),
                call.payloadHash());
        byte[] body = call.body().bytes();
        try {
            byte[] result = write
                    ? operations.write(call.principalId(), call.operationId(), call.method(), call.payloadHash(),
                            () -> invokeObject(handle, context, body))
                    : invokeObject(handle, context, body);
            return new IpcMessage.Result(appId, call.operationId(), new IpcMessage.Body(result));
        } catch (AppException e) {
            return error(e.code(), e.getMessage(), e.retryable());
        } catch (DataIntegrityViolationException e) {
            return error(ErrorCode.OPERATION_PENDING, "operation is being written by another request", true);
        } catch (RuntimeException e) {
            return error(ErrorCode.INTERNAL, "method failed", false);
        }
    }

    private void answerSubscribe(String requestId, IpcMessage.Subscribe subscribe) {
        if (!appId.equals(subscribe.appId())) {
            respond(requestId, error(ErrorCode.FORBIDDEN, "application is not served by this backend", false));
            return;
        }
        String topic = subscribe.topic();
        List<IlmekEvent> batch;
        long start;
        try {
            batch = events.since(topic, subscribe.cursor());
            for (IlmekEvent event : batch) {
                if (!isSendableObject(event.getBody())) {
                    throw new AppException(ErrorCode.INTERNAL, "event body must be a JSON object", false);
                }
            }
            start = subscribe.cursor() == null ? 0L
                    : EventCursor.decode(subscribe.cursor()).map(EventCursor.Parsed::sequence).orElse(0L);
        } catch (AppException e) {
            respond(requestId, error(e.code(), e.getMessage(), e.retryable()));
            return;
        } catch (IllegalArgumentException e) {
            respond(requestId, error(ErrorCode.INVALID_MESSAGE, "invalid cursor or topic", false));
            return;
        } catch (RuntimeException e) {
            respond(requestId, error(ErrorCode.INTERNAL, "subscription failed", false));
            return;
        }
        String subscriptionId = newUuidV7();
        respond(requestId, new IpcMessage.Subscribed(subscriptionId, appId, topic,
                EventCursor.encode(topic, start), EVENT_WINDOW));
        for (IlmekEvent event : batch) {
            respond(requestId, new IpcMessage.Event(subscriptionId, appId, topic, event.getSequence(),
                    event.getEventId(), EventCursor.encode(topic, event.getSequence()),
                    new IpcMessage.Body(event.getBody())));
        }
    }

    private static byte[] invokeObject(IlmekMethodRegistry.MethodHandle handle, IlmekCallContext context,
                                       byte[] body) {
        byte[] result = handle.invoke().apply(context, body);
        if (!isSendableObject(result)) {
            throw new AppException(ErrorCode.INTERNAL, "method result must be a JSON object", false);
        }
        return result;
    }

    private void respond(String requestId, IpcMessage message) {
        byte[] frame;
        try {
            frame = IpcCodec.encode(message, requestId, communityId);
        } catch (IpcFormatException | FrameFormatException e) {
            frame = IpcCodec.encode(error(ErrorCode.INTERNAL, "response could not be encoded", false),
                    requestId, communityId);
        }
        enqueue(frame);
    }

    private void enqueue(byte[] frame) {
        while (!closed.get()) {
            try {
                if (outbound.offer(frame, 1, TimeUnit.SECONDS)) {
                    return;
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
        }
    }

    private static Frame readFrame(Socket socket, InputStream in, FrameDecoder decoder, byte[] buffer,
                                   long deadlineNanos) throws IOException {
        while (true) {
            Optional<Frame> frame = decoder.nextFrame();
            if (frame.isPresent()) {
                return frame.get();
            }
            if (deadlineNanos != NO_DEADLINE) {
                long remaining = deadlineNanos - System.nanoTime();
                if (remaining <= 0) {
                    throw new SocketTimeoutException("IPC_READY deadline exceeded");
                }
                socket.setSoTimeout((int) Math.max(1L, TimeUnit.NANOSECONDS.toMillis(remaining)));
            }
            int n = in.read(buffer);
            if (n < 0) {
                return null;
            }
            decoder.push(Arrays.copyOf(buffer, n));
        }
    }

    private static boolean isSendableObject(byte[] bytes) {
        if (bytes == null || bytes.length > MAX_BODY_BYTES) {
            return false;
        }
        try {
            String text = StandardCharsets.UTF_8.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes))
                    .toString();
            JsonNode node = JSON.readTree(text);
            return node != null && node.isObject();
        } catch (IOException e) {
            return false;
        }
    }

    private static IpcMessage error(ErrorCode code, String message, boolean retryable) {
        return new IpcMessage.Error(code, bounded(message), retryable, null, Map.of());
    }

    private static String bounded(String message) {
        if (message == null) {
            return "";
        }
        if (message.length() <= MAX_MESSAGE_CHARS) {
            return message;
        }
        int end = MAX_MESSAGE_CHARS;
        if (Character.isHighSurrogate(message.charAt(end - 1))) {
            end--;
        }
        return message.substring(0, end);
    }

    private static String newUuidV7() {
        long unixMillis = System.currentTimeMillis();
        long randA = ThreadLocalRandom.current().nextLong() & 0xFFFL;
        long randB = ThreadLocalRandom.current().nextLong() & 0x3FFFFFFFFFFFFFFFL;
        long msb = (unixMillis << 16) | 0x7000L | randA;
        long lsb = 0x8000000000000000L | randB;
        return new UUID(msb, lsb).toString();
    }

    private static void closeQuietly(Socket socket) {
        try {
            socket.close();
        } catch (IOException ignored) {
            // the socket is being discarded; nothing further can be done with a failed close
        }
    }
}
