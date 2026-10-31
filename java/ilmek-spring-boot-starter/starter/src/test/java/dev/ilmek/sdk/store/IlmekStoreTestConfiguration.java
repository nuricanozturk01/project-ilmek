package dev.ilmek.sdk.store;

import org.springframework.boot.autoconfigure.domain.EntityScan;
import org.springframework.boot.SpringBootConfiguration;
import org.springframework.data.jpa.repository.config.EnableJpaRepositories;

@SpringBootConfiguration
@EntityScan(basePackageClasses = IlmekOperation.class)
@EnableJpaRepositories(basePackageClasses = IlmekOperationRepository.class)
public class IlmekStoreTestConfiguration {
}
