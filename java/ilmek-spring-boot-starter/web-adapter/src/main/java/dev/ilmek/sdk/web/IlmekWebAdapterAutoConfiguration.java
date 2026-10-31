package dev.ilmek.sdk.web;

import dev.ilmek.sdk.autoconfigure.IlmekAutoConfiguration;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.springframework.beans.factory.SmartInitializingSingleton;
import org.springframework.boot.autoconfigure.AutoConfiguration;
import org.springframework.boot.autoconfigure.condition.ConditionalOnClass;
import org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean;
import org.springframework.boot.autoconfigure.condition.ConditionalOnWebApplication;
import org.springframework.boot.autoconfigure.condition.ConditionalOnWebApplication.Type;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.web.context.WebApplicationContext;
import org.springframework.web.servlet.DispatcherServlet;

@AutoConfiguration(after = IlmekAutoConfiguration.class)
@ConditionalOnWebApplication(type = Type.SERVLET)
@ConditionalOnClass({DispatcherServlet.class, MockMvc.class})
public class IlmekWebAdapterAutoConfiguration {

    private static final Log LOG = LogFactory.getLog(IlmekWebAdapterAutoConfiguration.class);

    @Bean
    @ConditionalOnMissingBean
    HttpBridge ilmekHttpBridge(WebApplicationContext context) {
        return new HttpBridge(context);
    }

    @Configuration(proxyBeanMethods = false)
    @ConditionalOnClass(name = "org.springframework.security.web.SecurityFilterChain")
    static class SecurityNotice {

        @Bean
        SmartInitializingSingleton ilmekWebSecurityNotice() {
            return () -> LOG.warn("Spring Security is on the classpath, but the ILMEK web adapter does not support it"
                    + " (no login, sessions or CSRF); X-Ilmek-Principal is not authenticated");
        }
    }
}
