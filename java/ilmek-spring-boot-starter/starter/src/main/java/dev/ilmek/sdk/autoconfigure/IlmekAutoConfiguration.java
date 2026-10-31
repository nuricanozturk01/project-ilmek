package dev.ilmek.sdk.autoconfigure;

import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.runtime.BackendConfig;
import dev.ilmek.sdk.runtime.ConfigException;
import dev.ilmek.sdk.runtime.IlmekBackendClient;
import dev.ilmek.sdk.store.IlmekEventFloorRepository;
import dev.ilmek.sdk.store.IlmekEventRepository;
import dev.ilmek.sdk.store.IlmekEventsService;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import dev.ilmek.sdk.store.IlmekOperationsService;
import java.io.IOException;
import java.io.InputStream;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.IntConsumer;
import java.util.function.Supplier;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.AutoConfiguration;
import org.springframework.boot.autoconfigure.condition.ConditionalOnBean;
import org.springframework.boot.autoconfigure.condition.ConditionalOnClass;
import org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean;
import org.springframework.boot.autoconfigure.data.jpa.JpaRepositoriesAutoConfiguration;
import org.springframework.boot.context.event.ApplicationReadyEvent;
import org.springframework.context.ApplicationContext;
import org.springframework.context.ApplicationListener;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

@AutoConfiguration(after = JpaRepositoriesAutoConfiguration.class)
@ConditionalOnClass(IlmekBackendClient.class)
public class IlmekAutoConfiguration {

    private static final String CLIENT_THREAD = "ilmek-backend-client";

    private static final String IPC_HOST = "127.0.0.1";
    private static final Duration CONFIG_DEADLINE = Duration.ofSeconds(10);
    private static final Duration HELLO_DEADLINE = Duration.ofSeconds(10);
    private static final int EXIT_CLEAN = 0;
    private static final int EXIT_FAILURE = 1;
    private static final Log LOG = LogFactory.getLog(IlmekAutoConfiguration.class);

    public interface ConfigInput extends Supplier<InputStream> {
    }

    @Bean
    @ConditionalOnMissingBean
    ConfigInput ilmekConfigInput() {
        return () -> System.in;
    }

    @Bean
    @ConditionalOnMissingBean
    IntConsumer ilmekProcessExit() {
        return System::exit;
    }

    @Bean
    @ConditionalOnMissingBean
    static IlmekMethodRegistry ilmekMethodRegistry() {
        return new IlmekMethodRegistry();
    }

    @Configuration(proxyBeanMethods = false)
    @ConditionalOnBean({IlmekOperationRepository.class, IlmekEventRepository.class, IlmekEventFloorRepository.class})
    static class IlmekStoreConfiguration {

        @Bean
        @ConditionalOnMissingBean
        IlmekOperationsService ilmekOperationsService(IlmekOperationRepository repository) {
            return new IlmekOperationsService(repository);
        }

        @Bean
        @ConditionalOnMissingBean
        IlmekEventsService ilmekEventsService(IlmekEventRepository repository,
                                              IlmekEventFloorRepository floors) {
            return new IlmekEventsService(repository, floors);
        }

        @Bean
        ApplicationListener<ApplicationReadyEvent> ilmekBackendLauncher(
                IlmekMethodRegistry registry,
                IlmekOperationsService operations,
                IlmekEventsService events,
                ConfigInput configInput,
                IntConsumer processExit) {
            AtomicBoolean started = new AtomicBoolean();
            return event -> {
                if (started.compareAndSet(false, true)) {
                    startClientThread(event.getApplicationContext(), configInput, registry, operations, events,
                            processExit);
                }
            };
        }
    }

    private static void startClientThread(ApplicationContext context, ConfigInput configInput,
                                          IlmekMethodRegistry registry, IlmekOperationsService operations,
                                          IlmekEventsService events, IntConsumer processExit) {
        Thread.ofPlatform().name(CLIENT_THREAD).daemon(false).start(
                () -> runClient(context, configInput, registry, operations, events, processExit));
    }

    private static void runClient(ApplicationContext context, ConfigInput configInput,
                                  IlmekMethodRegistry registry, IlmekOperationsService operations,
                                  IlmekEventsService events, IntConsumer processExit) {
        BackendConfig config;
        try {
            config = BackendConfig.readFrom(configInput.get(), CONFIG_DEADLINE);
        } catch (ConfigException e) {
            if (e.isAbsent()) {
                LOG.info("No ILMEK backend configuration on standard input; backend client not started ("
                        + e.getMessage() + ")");
                return;
            }
            LOG.warn("ILMEK backend configuration is invalid; stopping");
            stop(context, processExit, EXIT_FAILURE);
            return;
        }
        int status;
        try {
            IlmekBackendClient client = new IlmekBackendClient(IPC_HOST, config.ipcPort(), config.token(),
                    config.appId(), config.communityId(), registry, operations, events);
            client.connectAndServe(HELLO_DEADLINE);
            status = EXIT_CLEAN;
        } catch (IOException | RuntimeException e) {
            LOG.error("ILMEK backend client stopped: " + e.getMessage(), e);
            status = EXIT_FAILURE;
        }
        stop(context, processExit, status);
    }

    private static void stop(ApplicationContext context, IntConsumer processExit, int status) {
        try {
            SpringApplication.exit(context);
        } finally {
            processExit.accept(status);
        }
    }
}
