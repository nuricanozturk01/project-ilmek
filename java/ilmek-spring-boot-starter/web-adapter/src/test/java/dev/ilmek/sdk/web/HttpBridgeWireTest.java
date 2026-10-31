package dev.ilmek.sdk.web;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import dev.ilmek.sdk.web.testapp.TaskController;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.Frame;
import dev.ilmek.sdk.wire.FrameDecoder;
import dev.ilmek.sdk.wire.IpcCodec;
import dev.ilmek.sdk.wire.IpcMessage;
import java.io.ByteArrayInputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Arrays;
import java.util.HashSet;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Assumptions;
import org.junit.jupiter.api.Test;
import org.springframework.context.ConfigurableApplicationContext;

class HttpBridgeWireTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();
    private static final String COMMUNITY = id(1);
    private static final String APP = id(2);
    private static final String PRINCIPAL = id(3);
    private static final String TOKEN = "tok";

    @Test
    void retriedWriteReplaysTheStoredResultAndRunsTheControllerOnce() throws Exception {
        BlockingQueue<Integer> exits = new LinkedBlockingQueue<>();
        try (FakeHost host = new FakeHost()) {
            ConfigurableApplicationContext context = TestRuntime.start(() -> stdin(host.port()), exits::add);
            try {
                TaskController controller = context.getBean(TaskController.class);
                host.handshake();

                String operation = id(100);
                String body = "{\"method\":\"POST\",\"path\":\"/api/tasks\",\"body\":{\"title\":\"once\"}}";
                host.send(call(operation, "a".repeat(64), body), id(101));
                IpcMessage.Result first = host.readResult(id(101));
                host.send(call(operation, "a".repeat(64), body), id(102));
                IpcMessage.Result retry = host.readResult(id(102));

                assertArrayEquals(first.body().bytes(), retry.body().bytes());
                assertEquals(201, status(first));
                assertEquals(1, controller.creations());

                host.send(call(operation, "b".repeat(64), body.replace("once", "other")), id(103));
                IpcCodec.Decoded conflict = host.read();
                assertEquals(ErrorCode.OPERATION_CONFLICT, assertInstanceOf(IpcMessage.Error.class,
                        conflict.message()).code());
                assertEquals(1, controller.creations());

                host.send(call(id(104), "c".repeat(64), body), id(105));
                host.readResult(id(105));
                assertEquals(2, controller.creations());
            } finally {
                host.disconnect();
                assertEquals(0, exits.poll(10, TimeUnit.SECONDS));
                context.close();
            }
        }
    }

    @Test
    void noPortIsBoundByDefaultButAUserPortIsHonoured() throws Exception {
        Assumptions.assumeTrue(ListeningPorts.available());
        try (ServerSocket control = new ServerSocket(0, 0, InetAddress.getLoopbackAddress())) {
            Set<Integer> before = ListeningPorts.ofThisProcess();
            assertTrue(before.contains(control.getLocalPort()), "listening-port probe cannot see a known listener");

            ConfigurableApplicationContext quiet = TestRuntime.start(TestRuntime.NO_CONFIG, status -> { });
            Set<Integer> withDefault = ListeningPorts.ofThisProcess();
            quiet.close();
            assertEquals(Set.of(), difference(withDefault, before));

            ConfigurableApplicationContext explicit = TestRuntime.start(TestRuntime.NO_CONFIG, status -> { },
                    "server.port=0");
            Set<Integer> withUserPort = ListeningPorts.ofThisProcess();
            String port = explicit.getEnvironment().getProperty("server.port");
            explicit.close();
            assertEquals("0", port);
            assertEquals(1, difference(withUserPort, before).size());
        }
    }

    private static Set<Integer> difference(Set<Integer> after, Set<Integer> before) {
        Set<Integer> added = new HashSet<>(after);
        added.removeAll(before);
        return added;
    }

    private static int status(IpcMessage.Result result) throws IOException {
        JsonNode node = MAPPER.readTree(result.body().bytes());
        return node.get("status").asInt();
    }

    private static IpcMessage.Call call(String operation, String hash, String body) {
        return new IpcMessage.Call(APP, "1.0.0", "http.write", operation, hash, PRINCIPAL,
                new IpcMessage.Body(body.getBytes(UTF_8)));
    }

    private static InputStream stdin(int port) {
        String json = "{\"token\":\"" + TOKEN + "\",\"ipc_port\":" + port + ",\"community_id\":\"" + COMMUNITY
                + "\",\"app_id\":\"" + APP + "\",\"data_dir\":\"data\"}";
        return new ByteArrayInputStream(json.getBytes(UTF_8));
    }

    private static String id(long n) {
        return String.format("0190f5a0-0000-7000-8000-%012d", n);
    }

    private static final class FakeHost implements AutoCloseable {

        private final ServerSocket server = new ServerSocket(0, 0, InetAddress.getLoopbackAddress());
        private final FrameDecoder decoder = new FrameDecoder();
        private Socket socket;
        private InputStream in;
        private OutputStream out;

        FakeHost() throws IOException {
        }

        int port() {
            return server.getLocalPort();
        }

        void handshake() throws IOException {
            server.setSoTimeout(20_000);
            socket = server.accept();
            socket.setSoTimeout(10_000);
            in = socket.getInputStream();
            out = socket.getOutputStream();
            IpcCodec.Decoded hello = read();
            assertInstanceOf(IpcMessage.Hello.class, hello.message());
            send(new IpcMessage.Ready(APP), hello.requestId());
        }

        IpcCodec.Decoded read() throws IOException {
            byte[] buffer = new byte[8192];
            while (true) {
                Optional<Frame> frame = decoder.nextFrame();
                if (frame.isPresent()) {
                    return IpcCodec.decodeWithHeader(frame.get().metadata(), frame.get().body());
                }
                int n = in.read(buffer);
                if (n < 0) {
                    throw new EOFException("client closed the connection");
                }
                decoder.push(Arrays.copyOf(buffer, n));
            }
        }

        IpcMessage.Result readResult(String requestId) throws IOException {
            IpcCodec.Decoded reply = read();
            assertEquals(requestId, reply.requestId());
            return assertInstanceOf(IpcMessage.Result.class, reply.message());
        }

        void send(IpcMessage message, String requestId) throws IOException {
            out.write(IpcCodec.encode(message, requestId, COMMUNITY));
            out.flush();
        }

        void disconnect() throws IOException {
            if (socket != null) {
                socket.close();
            }
        }

        @Override
        public void close() throws IOException {
            disconnect();
            server.close();
        }
    }
}
