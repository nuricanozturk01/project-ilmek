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
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.ArrayDeque;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Semaphore;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import org.springframework.dao.DataIntegrityViolationException;

public final class IlmekBackendClient {

    private static final long NO_DEADLINE = Long.MAX_VALUE;
    private static final long SLOW_CONSUMER_NANOS = TimeUnit.SECONDS.toNanos(30);
    private static final long POLL_MILLIS = 250;
    private static final int READ_BUFFER_BYTES = 8_192;
    private static final int MAX_IN_FLIGHT = 64;
    private static final int MAX_SUBSCRIPTIONS = 8;
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
    private final Map<String, Subscription> subscriptions = new ConcurrentHashMap<>();
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
                IpcMessage message = decoded.message();
                if (isCommunityScoped(message) && !communityId.equals(decoded.communityId())) {
                    respond(decoded.requestId(), error(ErrorCode.FORBIDDEN, "community is not served by this backend", false));
                    continue;
                }
                switch (message) {
                    case IpcMessage.Call call -> dispatch(executor,
                            () -> respond(decoded.requestId(), answerCall(call)));
                    case IpcMessage.Subscribe subscribe -> openSubscription(executor, decoded.requestId(), subscribe);
                    case IpcMessage.EventAck ack -> respond(decoded.requestId(), acknowledge(ack));
                    case IpcMessage.Unsubscribe unsubscribe ->
                            respond(decoded.requestId(), unsubscribe(unsubscribe));
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
            subscriptions.values().forEach(Subscription::cancel);
            executor.shutdownNow();
        }
    }

    private static boolean isCommunityScoped(IpcMessage message) {
        return message instanceof IpcMessage.Call || message instanceof IpcMessage.Subscribe
                || message instanceof IpcMessage.EventAck || message instanceof IpcMessage.Unsubscribe;
    }

    private void dispatch(ExecutorService executor, Runnable task) throws InterruptedException {
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

    private void openSubscription(ExecutorService executor, String requestId, IpcMessage.Subscribe subscribe) {
        if (!appId.equals(subscribe.appId())) {
            respond(requestId, error(ErrorCode.FORBIDDEN, "application is not served by this backend", false));
            return;
        }
        if (subscriptions.size() >= MAX_SUBSCRIPTIONS) {
            respond(requestId, error(ErrorCode.RESOURCE_EXHAUSTED, "too many subscriptions", true));
            return;
        }
        String topic = subscribe.topic();
        long start;
        try {
            if (subscribe.cursor() == null) {
                start = events.latestSequence(topic);
            } else {
                start = EventCursor.decode(subscribe.cursor()).map(EventCursor.Parsed::sequence)
                        .orElseThrow(() -> new IllegalArgumentException("invalid cursor"));
                events.since(topic, subscribe.cursor());
            }
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
        Subscription subscription = new Subscription(newUuidV7(), topic, requestId, start);
        respond(requestId, new IpcMessage.Subscribed(subscription.id, appId, topic,
                EventCursor.encode(topic, start), EVENT_WINDOW));
        subscriptions.put(subscription.id, subscription);
        executor.execute(() -> stream(subscription));
    }

    private void stream(Subscription subscription) {
        try {
            while (subscription.isActive() && !closed.get()) {
                long position = subscription.position();
                if (events.latestSequence(subscription.topic) <= position) {
                    subscription.pause(POLL_MILLIS);
                    continue;
                }
                List<IlmekEvent> batch = events.since(subscription.topic,
                        EventCursor.encode(subscription.topic, position));
                for (IlmekEvent event : batch) {
                    if (!subscription.awaitWindowSlot()) {
                        return;
                    }
                    if (!isSendableObject(event.getBody())) {
                        respond(subscription.requestId,
                                error(ErrorCode.INTERNAL, "event body must be a JSON object", false));
                        return;
                    }
                    subscription.recordSent(event.getSequence());
                    respond(subscription.requestId, new IpcMessage.Event(subscription.id, appId,
                            subscription.topic, event.getSequence(), event.getEventId(),
                            EventCursor.encode(subscription.topic, event.getSequence()),
                            new IpcMessage.Body(event.getBody())));
                }
            }
        } catch (AppException e) {
            respond(subscription.requestId, error(e.code(), e.getMessage(), e.retryable()));
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } catch (RuntimeException e) {
            respond(subscription.requestId, error(ErrorCode.INTERNAL, "subscription failed", false));
        } finally {
            subscription.cancel();
            subscriptions.remove(subscription.id);
        }
    }

    private IpcMessage acknowledge(IpcMessage.EventAck ack) {
        Subscription subscription = subscriptions.get(ack.subscriptionId());
        if (subscription == null) {
            return error(ErrorCode.NOT_FOUND, "unknown subscription", false);
        }
        if (ack.sequence() > subscription.position()
                || !ack.cursor().equals(EventCursor.encode(subscription.topic, ack.sequence()))) {
            return error(ErrorCode.INVALID_MESSAGE, "acknowledgement is ahead of delivery or has a foreign cursor",
                    false);
        }
        subscription.acknowledge(ack.sequence());
        return new IpcMessage.EventAcked(ack.subscriptionId(), ack.sequence());
    }

    private IpcMessage unsubscribe(IpcMessage.Unsubscribe unsubscribe) {
        Subscription subscription = subscriptions.remove(unsubscribe.subscriptionId());
        if (subscription != null) {
            subscription.cancel();
        }
        return new IpcMessage.Unsubscribed(unsubscribe.subscriptionId());
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
        }
    }

    private static final class Subscription {

        private final String id;
        private final String topic;
        private final String requestId;
        private final ArrayDeque<Long> unacknowledged = new ArrayDeque<>();
        private long position;
        private long progressDeadlineNanos;
        private boolean active = true;

        Subscription(String id, String topic, String requestId, long start) {
            this.id = id;
            this.topic = topic;
            this.requestId = requestId;
            this.position = start;
        }

        synchronized long position() {
            return position;
        }

        synchronized boolean isActive() {
            return active;
        }

        synchronized void recordSent(long sequence) {
            position = sequence;
            unacknowledged.addLast(sequence);
        }

        synchronized void acknowledge(long sequence) {
            while (!unacknowledged.isEmpty() && unacknowledged.peekFirst() <= sequence) {
                unacknowledged.pollFirst();
            }
            progressDeadlineNanos = System.nanoTime() + SLOW_CONSUMER_NANOS;
            notifyAll();
        }

        synchronized boolean awaitWindowSlot() throws InterruptedException {
            if (unacknowledged.size() >= EVENT_WINDOW) {
                progressDeadlineNanos = System.nanoTime() + SLOW_CONSUMER_NANOS;
            }
            while (active && unacknowledged.size() >= EVENT_WINDOW) {
                long remaining = progressDeadlineNanos - System.nanoTime();
                if (remaining <= 0) {
                    throw new AppException(ErrorCode.SLOW_CONSUMER, "no acknowledgement progress", false);
                }
                TimeUnit.NANOSECONDS.timedWait(this, remaining);
            }
            return active;
        }

        synchronized void pause(long millis) throws InterruptedException {
            if (active) {
                wait(millis);
            }
        }

        synchronized void cancel() {
            active = false;
            notifyAll();
        }
    }
}
