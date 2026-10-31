package dev.ilmek.examples.tasks;

import dev.ilmek.sdk.store.IlmekOperation;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.boot.autoconfigure.domain.EntityScan;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

// Starter store beans exist only when these packages are scanned; see the starter's IlmekAutoConfiguration.
@SpringBootApplication
@EntityScan(basePackageClasses = {TasksApplication.class, IlmekOperation.class})
@EnableJpaRepositories(basePackageClasses = {TasksApplication.class, IlmekOperationRepository.class})
public class TasksApplication {

    public static void main(String[] args) {
        SpringApplication application = new SpringApplication(TasksApplication.class);
        // Without a web server nothing else keeps the JVM alive; the starter's IPC thread is a daemon.
        application.setKeepAlive(true);
        application.run(args);
    }
}
