package dev.ilmek.sdk.store;

import java.util.stream.Stream;
import org.springframework.boot.autoconfigure.AutoConfigurations;
import org.springframework.boot.data.jpa.autoconfigure.DataJpaRepositoriesAutoConfiguration;
import org.springframework.boot.hibernate.autoconfigure.HibernateJpaAutoConfiguration;
import org.springframework.boot.jdbc.autoconfigure.DataSourceAutoConfiguration;
import org.springframework.boot.transaction.autoconfigure.TransactionAutoConfiguration;

public final class JpaAutoConfigurations {

    private JpaAutoConfigurations() {
    }

    public static AutoConfigurations with(Class<?> extra) {
        return AutoConfigurations.of(Stream.concat(Stream.of(
                DataSourceAutoConfiguration.class,
                HibernateJpaAutoConfiguration.class,
                DataJpaRepositoriesAutoConfiguration.class,
                TransactionAutoConfiguration.class), Stream.of(extra)).toArray(Class<?>[]::new));
    }
}
