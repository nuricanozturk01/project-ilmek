package dev.ilmek.sdk.store;

import java.util.stream.Stream;
import org.springframework.boot.autoconfigure.AutoConfigurations;
import org.springframework.boot.autoconfigure.data.jpa.JpaRepositoriesAutoConfiguration;
import org.springframework.boot.autoconfigure.jdbc.DataSourceAutoConfiguration;
import org.springframework.boot.autoconfigure.orm.jpa.HibernateJpaAutoConfiguration;
import org.springframework.boot.autoconfigure.transaction.TransactionAutoConfiguration;

public final class JpaAutoConfigurations {

    private JpaAutoConfigurations() {
    }

    public static AutoConfigurations with(Class<?> extra) {
        return AutoConfigurations.of(Stream.concat(Stream.of(
                DataSourceAutoConfiguration.class,
                HibernateJpaAutoConfiguration.class,
                JpaRepositoriesAutoConfiguration.class,
                TransactionAutoConfiguration.class), Stream.of(extra)).toArray(Class<?>[]::new));
    }
}
