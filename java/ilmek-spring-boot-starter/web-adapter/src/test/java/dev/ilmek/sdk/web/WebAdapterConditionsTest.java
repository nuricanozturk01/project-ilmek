package dev.ilmek.sdk.web;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import dev.ilmek.sdk.api.IlmekMethodRegistry;
import dev.ilmek.sdk.autoconfigure.IlmekAutoConfiguration;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.autoconfigure.AutoConfigurations;
import org.springframework.boot.test.context.FilteredClassLoader;
import org.springframework.boot.test.context.runner.ApplicationContextRunner;
import org.springframework.boot.test.context.runner.ReactiveWebApplicationContextRunner;
import org.springframework.boot.test.context.runner.WebApplicationContextRunner;
import org.springframework.boot.test.system.CapturedOutput;
import org.springframework.boot.test.system.OutputCaptureExtension;
import org.springframework.boot.SpringApplication;
import org.springframework.core.env.StandardEnvironment;
import org.springframework.mock.env.MockPropertySource;
import org.springframework.security.web.SecurityFilterChain;

@ExtendWith(OutputCaptureExtension.class)
class WebAdapterConditionsTest {

    private static final AutoConfigurations CONFIGURATIONS =
            AutoConfigurations.of(IlmekAutoConfiguration.class, IlmekWebAdapterAutoConfiguration.class);
    private static final String NOTICE = "does not support it";

    @Test
    void registersBothMethodsInAServletApplication() {
        new WebApplicationContextRunner().withConfiguration(CONFIGURATIONS).run(context -> {
            IlmekMethodRegistry registry = context.getBean(IlmekMethodRegistry.class);
            assertTrue(registry.find("http.read").isPresent());
            assertTrue(registry.find("http.write").isPresent());
        });
    }

    @Test
    void backsOffInANonWebApplication() {
        new ApplicationContextRunner().withConfiguration(CONFIGURATIONS).run(context -> {
            assertFalse(context.containsBean("ilmekHttpBridge"));
            assertTrue(context.getBean(IlmekMethodRegistry.class).find("http.read").isEmpty());
        });
    }

    @Test
    void backsOffInAReactiveApplication() {
        new ReactiveWebApplicationContextRunner().withConfiguration(CONFIGURATIONS).run(context -> {
            assertFalse(context.containsBean("ilmekHttpBridge"));
            assertTrue(context.getBean(IlmekMethodRegistry.class).find("http.write").isEmpty());
        });
    }

    @Test
    void warnsOnceWhenSpringSecurityIsOnTheClasspath(CapturedOutput output) {
        new WebApplicationContextRunner().withConfiguration(CONFIGURATIONS).run(context -> { });
        assertEquals(1, output.getAll().split(NOTICE, -1).length - 1);
    }

    @Test
    void staysQuietWithoutSpringSecurity(CapturedOutput output) {
        new WebApplicationContextRunner().withConfiguration(CONFIGURATIONS)
                .withClassLoader(new FilteredClassLoader(SecurityFilterChain.class))
                .run(context -> { });
        assertFalse(output.getAll().contains(NOTICE));
    }

    @Test
    void postProcessorLeavesNonServletApplicationsAlone() {
        for (WebApplicationType type : new WebApplicationType[] {WebApplicationType.NONE, WebApplicationType.REACTIVE}) {
            StandardEnvironment environment = new StandardEnvironment();
            new IlmekNoServerPortPostProcessor().postProcessEnvironment(environment, application(type));
            assertFalse(environment.containsProperty("server.port"));
        }
    }

    @Test
    void postProcessorRespectsAnExplicitPort() {
        StandardEnvironment environment = new StandardEnvironment();
        environment.getPropertySources().addFirst(new MockPropertySource().withProperty("server.port", "8080"));
        new IlmekNoServerPortPostProcessor().postProcessEnvironment(environment,
                application(WebApplicationType.SERVLET));
        assertEquals("8080", environment.getProperty("server.port"));
    }

    private static SpringApplication application(WebApplicationType type) {
        SpringApplication application = new SpringApplication(Object.class);
        application.setWebApplicationType(type);
        return application;
    }
}
