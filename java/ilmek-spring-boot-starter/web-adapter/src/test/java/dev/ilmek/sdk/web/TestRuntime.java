package dev.ilmek.sdk.web;

import dev.ilmek.sdk.autoconfigure.IlmekAutoConfiguration.ConfigInput;
import dev.ilmek.sdk.web.testapp.TestApplication;
import java.io.ByteArrayInputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.function.IntConsumer;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;

final class TestRuntime {

    static final ConfigInput NO_CONFIG = () -> new ByteArrayInputStream(new byte[0]);

    private TestRuntime() {
    }

    static ConfigurableApplicationContext start(ConfigInput input, IntConsumer exit, String... extraProperties) {
        List<String> properties = new ArrayList<>(List.of(
                "spring.datasource.url=jdbc:h2:mem:" + UUID.randomUUID() + ";DB_CLOSE_DELAY=-1",
                "spring.jpa.hibernate.ddl-auto=create-drop",
                "spring.main.banner-mode=off"));
        properties.addAll(List.of(extraProperties));
        return new SpringApplicationBuilder(TestApplication.class)
                .web(WebApplicationType.SERVLET)
                .properties(properties.toArray(String[]::new))
                .initializers(context -> {
                    context.getBeanFactory().registerSingleton("ilmekConfigInput", input);
                    context.getBeanFactory().registerSingleton("ilmekProcessExit", exit);
                })
                .run();
    }
}
