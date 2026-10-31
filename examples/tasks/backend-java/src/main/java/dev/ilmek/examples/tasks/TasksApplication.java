package dev.ilmek.examples.tasks;

import dev.ilmek.sdk.store.IlmekOperation;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.AutoConfigurationPackage;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

// The starter store beans exist only when these packages are scanned.
@SpringBootApplication
@AutoConfigurationPackage(basePackageClasses = {TasksApplication.class, IlmekOperation.class})
@EnableJpaRepositories(basePackageClasses = {TasksApplication.class, IlmekOperationRepository.class})
public class TasksApplication {

    public static void main(String[] args) {
        new SpringApplication(TasksApplication.class).run(args);
    }
}
