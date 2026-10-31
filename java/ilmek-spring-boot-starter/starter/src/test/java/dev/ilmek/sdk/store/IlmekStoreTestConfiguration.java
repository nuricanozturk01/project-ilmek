package dev.ilmek.sdk.store;

import org.springframework.boot.SpringBootConfiguration;
import org.springframework.boot.autoconfigure.AutoConfigurationPackage;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

@SpringBootConfiguration
@AutoConfigurationPackage(basePackageClasses = IlmekOperation.class)
@EnableJpaRepositories(basePackageClasses = IlmekOperationRepository.class)
public class IlmekStoreTestConfiguration {
}
