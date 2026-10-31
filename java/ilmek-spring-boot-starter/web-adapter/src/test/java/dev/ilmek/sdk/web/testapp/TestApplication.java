package dev.ilmek.sdk.web.testapp;

import dev.ilmek.sdk.store.IlmekOperation;
import dev.ilmek.sdk.store.IlmekOperationRepository;
import org.springframework.boot.autoconfigure.AutoConfigurationPackage;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

@SpringBootApplication
@AutoConfigurationPackage(basePackageClasses = {TestApplication.class, IlmekOperation.class})
@EnableJpaRepositories(basePackageClasses = {TestApplication.class, IlmekOperationRepository.class})
public class TestApplication {
}
