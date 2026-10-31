package dev.ilmek.examples.tasks;

import static org.assertj.core.api.Assertions.assertThat;
import static org.junit.jupiter.api.Assertions.assertTrue;

import dev.ilmek.sdk.api.IlmekMethodRegistry;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.ApplicationContext;
import org.springframework.context.ApplicationListener;

@SpringBootTest
class TasksApplicationTest {

    @Autowired
    private ApplicationContext context;

    @Test
    void starterBackendLauncherIsRunningInContext() {
        assertThat(context.getBeansOfType(ApplicationListener.class)).containsKey("ilmekBackendLauncher");
        IlmekMethodRegistry registry = context.getBean(IlmekMethodRegistry.class);
        assertTrue(registry.find("tasks.list").isPresent());
        assertTrue(registry.find("tasks.create").isPresent());
        assertTrue(registry.find("tasks.complete").isPresent());
    }
}
