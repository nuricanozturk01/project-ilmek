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
import java.util.function.Supplier;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.springframework.boot.autoconfigure.AutoConfiguration;
import org.springframework.boot.autoconfigure.condition.ConditionalOnBean;
import org.springframework.boot.autoconfigure.condition.ConditionalOnClass;
import org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean;
import org.springframework.boot.autoconfigure.data.jpa.JpaRepositoriesAutoConfiguration;
import org.springframework.boot.context.event.ApplicationReadyEvent;
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
                ConfigInput configInput) {
            return event -> startClientThread(configInput, registry, operations, events);
        }
    }

    private static void startClientThread(ConfigInput configInput, IlmekMethodRegistry registry,
                                          IlmekOperationsService operations, IlmekEventsService events) {
        Thread thread = new Thread(
                () -> runClient(configInput, registry, operations, events), CLIENT_THREAD);
        thread.setDaemon(true);
        thread.start();
    }

    private static void runClient(ConfigInput configInput, IlmekMethodRegistry registry,
                                  IlmekOperationsService operations, IlmekEventsService events) {
        BackendConfig config;
        try {
            config = BackendConfig.readFrom(configInput.get(), CONFIG_DEADLINE);
        } catch (ConfigException e) {
            LOG.info("No ILMEK backend configuration on standard input; backend client not started ("
                    + e.getMessage() + ")");
            return;
        }
        try {
            IlmekBackendClient client = new IlmekBackendClient(IPC_HOST, config.ipcPort(), config.token(),
                    config.appId(), config.communityId(), registry, operations, events);
            client.connectAndServe(HELLO_DEADLINE);
        } catch (IOException | IllegalArgumentException e) {
            LOG.error("ILMEK backend client stopped: " + e.getMessage(), e);
        }
    }
}
