package dev.ilmek.sdk.web;

import java.util.Map;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.env.EnvironmentPostProcessor;
import org.springframework.core.Ordered;
import org.springframework.core.env.ConfigurableEnvironment;
import org.springframework.core.env.MapPropertySource;

// Registered under the pre-4.0 interface key; verified to still be honoured on Boot 4.1 (see the module tests).
public class IlmekNoServerPortPostProcessor implements EnvironmentPostProcessor, Ordered {

    static final String SOURCE_NAME = "ilmekWebAdapterDefaults";
    private static final String SERVER_PORT = "server.port";
    private static final String NO_HTTP_ENDPOINTS = "-1";

    @Override
    public void postProcessEnvironment(ConfigurableEnvironment environment, SpringApplication application) {
        if (application.getWebApplicationType() != WebApplicationType.SERVLET
                || environment.containsProperty(SERVER_PORT)) {
            return;
        }
        environment.getPropertySources()
                .addLast(new MapPropertySource(SOURCE_NAME, Map.of(SERVER_PORT, NO_HTTP_ENDPOINTS)));
    }

    @Override
    public int getOrder() {
        return Ordered.LOWEST_PRECEDENCE;
    }
}
