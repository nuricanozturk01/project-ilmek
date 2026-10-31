package dev.ilmek.sdk.api;

import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.HashMap;
import java.util.Map;
import java.util.Optional;
import java.util.function.BiFunction;
import org.springframework.beans.BeansException;
import org.springframework.beans.factory.config.BeanPostProcessor;
import org.springframework.util.ClassUtils;

public class IlmekMethodRegistry implements BeanPostProcessor {

    private static final String NAME_GRAMMAR = "[a-z][a-z0-9_.]{0,63}";

    public record MethodHandle(MethodKind kind, BiFunction<IlmekCallContext, byte[], byte[]> invoke) {
    }

    private final Map<String, MethodHandle> methods = new HashMap<>();
    private final Map<String, String> owners = new HashMap<>();

    public Optional<MethodHandle> find(String methodName) {
        return Optional.ofNullable(methods.get(methodName));
    }

    @Override
    public Object postProcessAfterInitialization(Object bean, String beanName) throws BeansException {
        Class<?> userClass = ClassUtils.getUserClass(bean);
        for (Method method : userClass.getDeclaredMethods()) {
            IlmekMethod annotation = method.getAnnotation(IlmekMethod.class);
            if (annotation == null) {
                continue;
            }
            String where = beanName + "." + method.getName();
            if (!annotation.name().matches(NAME_GRAMMAR)) {
                throw new IllegalStateException(
                        "invalid ILMEK method name '" + annotation.name() + "' on " + where);
            }
            validateSignature(method, where);
            String previous = owners.putIfAbsent(annotation.name(), where);
            if (previous != null) {
                throw new IllegalStateException(
                        "duplicate ILMEK method '" + annotation.name() + "' on " + where
                                + " (already registered by " + previous + ")");
            }
            method.setAccessible(true);
            methods.put(annotation.name(), new MethodHandle(annotation.kind(), invoker(bean, method)));
        }
        return bean;
    }

    private static void validateSignature(Method method, String where) {
        boolean valid = !Modifier.isStatic(method.getModifiers())
                && method.getReturnType() == byte[].class
                && method.getParameterCount() == 2
                && method.getParameterTypes()[0] == IlmekCallContext.class
                && method.getParameterTypes()[1] == byte[].class;
        if (!valid) {
            throw new IllegalStateException(
                    "unsupported signature for ILMEK method on " + where
                            + ": expected instance method byte[] (IlmekCallContext, byte[])");
        }
    }

    private static BiFunction<IlmekCallContext, byte[], byte[]> invoker(Object bean, Method method) {
        return (ctx, body) -> {
            try {
                return (byte[]) method.invoke(bean, ctx, body);
            } catch (InvocationTargetException e) {
                Throwable cause = e.getCause();
                if (cause instanceof RuntimeException runtime) {
                    throw runtime;
                }
                if (cause instanceof Error error) {
                    throw error;
                }
                throw new AppException(ErrorCode.INTERNAL, "method failed", false);
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("cannot invoke " + method, e);
            }
        };
    }
}
