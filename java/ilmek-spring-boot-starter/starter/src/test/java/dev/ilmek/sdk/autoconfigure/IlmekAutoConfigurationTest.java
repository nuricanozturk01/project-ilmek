package dev.ilmek.sdk.autoconfigure;

import static java.nio.charset.StandardCharsets.UTF_8;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.autoconfigure.app.AppNote;
import dev.ilmek.sdk.autoconfigure.app.AppNoteRepository;
import dev.ilmek.sdk.store.IlmekEventRepository;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import dev.ilmek.sdk.store.IlmekOperationsService;
import dev.ilmek.sdk.store.IlmekStoreTestConfiguration;
import dev.ilmek.sdk.wire.Frame;
import dev.ilmek.sdk.wire.FrameDecoder;
import dev.ilmek.sdk.wire.IpcCodec;
import dev.ilmek.sdk.wire.IpcMessage;
import jakarta.persistence.EntityManagerFactory;
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.io.InputStream;
import java.lang.reflect.Proxy;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.time.Duration;
import java.util.Arrays;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.function.IntConsumer;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.NoSuchBeanDefinitionException;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.AutoConfigurationPackage;
import org.springframework.boot.autoconfigure.AutoConfigurations;
import org.springframework.boot.autoconfigure.data.jpa.JpaRepositoriesAutoConfiguration;
import org.springframework.boot.autoconfigure.domain.EntityScan;
import org.springframework.boot.autoconfigure.jdbc.DataSourceAutoConfiguration;
import org.springframework.boot.autoconfigure.orm.jpa.HibernateJpaAutoConfiguration;
import org.springframework.boot.autoconfigure.transaction.TransactionAutoConfiguration;
import org.springframework.boot.context.event.ApplicationReadyEvent;
import org.springframework.boot.test.context.runner.ApplicationContextRunner;
import org.springframework.context.ApplicationListener;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

class IlmekAutoConfigurationTest {

    private static final String COMMUNITY = id(1);
    private static final String APP = id(2);
    private static final String TOKEN = "tok";
    private static final String CLIENT_THREAD = "ilmek-backend-client";

    private final BlockingQueue<Integer> exits = new LinkedBlockingQueue<>();

    private final String database = "ilmek-" + UUID.randomUUID();

    private final ApplicationContextRunner runner = new ApplicationContextRunner()
            .withConfiguration(AutoConfigurations.of(
                    DataSourceAutoConfiguration.class,
                    HibernateJpaAutoConfiguration.class,
                    JpaRepositoriesAutoConfiguration.class,
                    TransactionAutoConfiguration.class,
                    IlmekAutoConfiguration.class))
            .withPropertyValues(
                    "spring.datasource.url=jdbc:h2:mem:" + database + ";DB_CLOSE_DELAY=-1",
                    "spring.jpa.hibernate.ddl-auto=create-drop")
            .withUserConfiguration(AutoConfigurationPackageHolder.class);

    private final ApplicationContextRunner consumerRunner = runner.withUserConfiguration(IlmekStoreTestConfiguration.class)
            .withBean("ilmekProcessExit", IntConsumer.class, () -> exits::add);

    @Test
    void registersStarterBeansWhenConsumerRegistersStoreRepositories() {
        consumerRunner.run(context -> {
            assertNotNull(context.getBean(IlmekMethodRegistry.class));
            assertNotNull(context.getBean(IlmekOperationsService.class));
            assertNotNull(context.getBean(IlmekEventsService.class));
            assertTrue(context.containsBean("ilmekBackendLauncher"));
            assertNull(context.getStartupFailure());
        });
    }

    @Test
    void doesNotActivateStoreWithoutConsumerRepositoryRegistration() {
        runner.run(context -> {
            assertFalse(context.containsBean("ilmekBackendLauncher"));
            assertThrows(NoSuchBeanDefinitionException.class,
                    () -> context.getBean(IlmekOperationsService.class));
            assertNotNull(context.getBean(IlmekMethodRegistry.class));
        });
    }

    @Test
    void defaultConfigurationSourceIsStandardInput() {
        runner.run(context -> {
            IlmekAutoConfiguration.ConfigInput source = context.getBean(IlmekAutoConfiguration.ConfigInput.class);
            assertSame(System.in, source.get());
        });
    }

    @Test
    void doesNotStartClientBeforeApplicationReady() throws IOException {
        try (FakeHost host = new FakeHost()) {
            consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class, () -> stdin(configJson(host.port())))
                    .run(context -> {
                        host.expectNoConnection(Duration.ofMillis(500));
                    });
        }
    }

    @Test
    void startsNonDaemonClientOnReadyAndKeepsLinkOpen() throws Exception {
        try (FakeHost host = new FakeHost()) {
            consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class, () -> stdin(configJson(host.port())))
                    .run(context -> {
                        publishReady(context);
                        host.accept();

                        IpcCodec.Decoded hello = host.read();
                        IpcMessage.Hello message = assertInstanceOf(IpcMessage.Hello.class, hello.message());
                        assertEquals(TOKEN, message.token());
                        assertEquals(APP, message.appId());
                        assertEquals(COMMUNITY, hello.communityId());

                        host.send(new IpcMessage.Ready(APP), hello.requestId());
                        Thread.sleep(500);

                        Thread client = findThread(CLIENT_THREAD).orElseThrow();
                        assertFalse(client.isDaemon());
                        assertTrue(exits.isEmpty());

                        host.disconnect();
                        awaitThreadEnd(CLIENT_THREAD);
                        assertEquals(0, exitStatus());
                    });
        }
    }

    @Test
    void absentConfigurationSkipsClientWithoutFailingContext() throws Exception {
        CountDownLatch read = new CountDownLatch(1);
        consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class, () -> () -> {
            read.countDown();
            return new ByteArrayInputStream(new byte[0]);
        }).run(context -> {
            publishReady(context);
            assertTrue(read.await(5, TimeUnit.SECONDS));
            awaitThreadEnd(CLIENT_THREAD);
            assertTrue(context.isRunning());
            assertNull(context.getStartupFailure());
            assertTrue(exits.isEmpty());
        });
    }

    @Test
    void invalidConfigurationStopsContextAndExitsWithStatusOne() throws Exception {
        consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class,
                        () -> stdin("{\"token\":\"tok\""))
                .run(context -> {
                    publishReady(context);
                    awaitThreadEnd(CLIENT_THREAD);
                    assertEquals(1, exitStatus());
                    assertFalse(context.isRunning());
                });
    }

    @Test
    void runtimeFailureInClientStopsWithStatusOne() throws Exception {
        consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class,
                        () -> stdin(configJson(1).replace("\"token\":\"" + TOKEN + "\"", "\"token\":null")))
                .run(context -> {
                    publishReady(context);
                    awaitThreadEnd(CLIENT_THREAD);
                    assertEquals(1, exitStatus());
                    assertFalse(context.isRunning());
                });
    }

    @Test
    void exitHookRunsEvenWhenContextExitThrows() throws Exception {
        int closedPort;
        try (ServerSocket probe = new ServerSocket(0, 0, InetAddress.getLoopbackAddress())) {
            closedPort = probe.getLocalPort();
        }
        ConfigurableApplicationContext failing = (ConfigurableApplicationContext) Proxy.newProxyInstance(
                getClass().getClassLoader(), new Class<?>[] {ConfigurableApplicationContext.class},
                (proxy, method, args) -> {
                    if (method.getName().equals("getBeansOfType")) {
                        throw new IllegalStateException("context cannot be exited");
                    }
                    return null;
                });
        consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class,
                        () -> stdin(configJson(closedPort)))
                .run(context -> {
                    ApplicationListener<ApplicationReadyEvent> launcher =
                            context.getBean("ilmekBackendLauncher", ApplicationListener.class);
                    launcher.onApplicationEvent(new ApplicationReadyEvent(
                            new SpringApplication(), new String[0], failing, Duration.ZERO));
                    awaitThreadEnd(CLIENT_THREAD);
                    assertEquals(1, exitStatus());
                });
    }

    @Test
    void secondReadyEventDoesNotStartSecondClient() throws Exception {
        try (FakeHost host = new FakeHost()) {
            consumerRunner.withBean("stdin", IlmekAutoConfiguration.ConfigInput.class, () -> stdin(configJson(host.port())))
                    .run(context -> {
                        publishReady(context);
                        publishReady(context);
                        host.accept();

                        IpcCodec.Decoded hello = host.read();
                        host.expectNoConnection(Duration.ofMillis(500));

                        host.send(new IpcMessage.Ready(APP), hello.requestId());
                        host.disconnect();
                        awaitThreadEnd(CLIENT_THREAD);
                        assertEquals(0, exitStatus());
                    });
        }
    }

    @Test
    void appEntitiesAndRepositoriesCoexistWithStarterStore() {
        consumerRunner.withUserConfiguration(AppStoreConfiguration.class).run(context -> {
            assertNotNull(context.getBean(AppNoteRepository.class));
            assertNotNull(context.getBean(IlmekOperationRepository.class));
            assertNotNull(context.getBean(IlmekEventRepository.class));
            EntityManagerFactory factory = context.getBean(EntityManagerFactory.class);
            assertTrue(factory.getMetamodel().getEntities().stream()
                    .anyMatch(e -> e.getJavaType() == AppNote.class));
            assertTrue(factory.getMetamodel().getEntities().stream()
                    .anyMatch(e -> e.getJavaType().getSimpleName().equals("IlmekOperation")));
        });
    }

    private int exitStatus() throws InterruptedException {
        Integer status = exits.poll(5, TimeUnit.SECONDS);
        if (status == null) {
            throw new AssertionError("process exit was not requested");
        }
        return status;
    }

    private static void publishReady(ConfigurableApplicationContext context) {
        context.publishEvent(new ApplicationReadyEvent(new SpringApplication(), new String[0], context, Duration.ZERO));
    }

    private static IlmekAutoConfiguration.ConfigInput stdin(String text) {
        return () -> new ByteArrayInputStream(text.getBytes(UTF_8));
    }

    private static String configJson(int port) {
        return "{\"token\":\"" + TOKEN + "\",\"ipc_port\":" + port + ",\"community_id\":\"" + COMMUNITY
                + "\",\"app_id\":\"" + APP + "\",\"data_dir\":\"data\"}";
    }

    private static Optional<Thread> findThread(String name) {
        return Thread.getAllStackTraces().keySet().stream()
                .filter(t -> t.getName().equals(name) && t.isAlive())
                .findFirst();
    }

    private static void awaitThreadEnd(String name) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10);
        while (findThread(name).isPresent()) {
            if (System.nanoTime() > deadline) {
                throw new AssertionError("thread " + name + " still alive");
            }
            Thread.sleep(20);
        }
    }

    private static String id(long n) {
        return String.format("0190f5a0-0000-7000-8000-%012d", n);
    }

    @AutoConfigurationPackage(basePackageClasses = IlmekAutoConfigurationTest.class)
    static class AutoConfigurationPackageHolder {
    }

    @EntityScan(basePackageClasses = AppNote.class)
    @EnableJpaRepositories(basePackageClasses = AppNoteRepository.class)
    static class AppStoreConfiguration {
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

        void expectNoConnection(Duration wait) throws IOException {
            server.setSoTimeout((int) wait.toMillis());
            try {
                server.accept().close();
                throw new AssertionError("client connected before ApplicationReadyEvent");
            } catch (SocketTimeoutException expected) {
                // no connection within the window
            } finally {
                server.setSoTimeout(0);
            }
        }

        void accept() throws IOException {
            socket = server.accept();
            socket.setSoTimeout(5_000);
            in = socket.getInputStream();
            out = socket.getOutputStream();
        }

        IpcCodec.Decoded read() throws IOException {
            while (true) {
                Optional<Frame> frame = decoder.nextFrame();
                if (frame.isPresent()) {
                    return IpcCodec.decodeWithHeader(frame.get().metadata(), frame.get().body());
                }
                byte[] buffer = new byte[8192];
                int n = in.read(buffer);
                if (n < 0) {
                    throw new IOException("client closed before sending a frame");
                }
                decoder.push(Arrays.copyOf(buffer, n));
            }
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
            if (socket != null) {
                socket.close();
            }
            server.close();
        }
    }
}
