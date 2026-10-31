package dev.ilmek.sdk.web;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.asyncDispatch;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.api.IlmekCallContext;
import dev.ilmek.sdk.api.IlmekMethod;
import dev.ilmek.sdk.wire.ErrorCode;
import dev.ilmek.sdk.wire.MethodKind;
import jakarta.servlet.Filter;
import jakarta.servlet.ServletException;
import java.io.IOException;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.springframework.boot.web.servlet.FilterRegistrationBean;
import org.springframework.core.annotation.AnnotationAwareOrderComparator;
import org.springframework.http.HttpMethod;
import org.springframework.http.MediaType;
import org.springframework.mock.web.MockHttpServletResponse;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.MvcResult;
import org.springframework.test.web.servlet.request.MockHttpServletRequestBuilder;
import org.springframework.test.web.servlet.request.MockMvcRequestBuilders;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import org.springframework.web.context.WebApplicationContext;

class HttpBridge {

    static final int MAX_PAYLOAD_BYTES = 200 * 1024;
    static final String PRINCIPAL_HEADER = "x-ilmek-principal";

    private static final Log LOG = LogFactory.getLog(HttpBridge.class);
    private static final Set<String> READ_METHODS = Set.of("GET", "HEAD", "OPTIONS");
    private static final Set<String> WRITE_METHODS = Set.of("POST", "PUT", "PATCH", "DELETE");
    private static final Set<String> DROPPED_HEADERS = Set.of("cookie", "authorization", "host", "content-length",
            PRINCIPAL_HEADER);
    private static final List<String> RETURNED_HEADERS = List.of("content-type", "location", "etag", "cache-control");

    private final WebApplicationContext context;
    private volatile MockMvc mockMvc;

    HttpBridge(WebApplicationContext context) {
        this.context = context;
    }

    @IlmekMethod(name = "http.read", kind = MethodKind.READ)
    byte[] read(IlmekCallContext call, byte[] body) {
        return handle(call, body, READ_METHODS, false);
    }

    @IlmekMethod(name = "http.write", kind = MethodKind.WRITE)
    byte[] write(IlmekCallContext call, byte[] body) {
        return handle(call, body, WRITE_METHODS, true);
    }

    private byte[] handle(IlmekCallContext call, byte[] request, Set<String> methods, boolean bodyAllowed) {
        if (request.length > MAX_PAYLOAD_BYTES) {
            return error(413, "payload too large");
        }
        HttpCall http = HttpCall.parse(request, methods, bodyAllowed);
        MockHttpServletResponse response = dispatch(call, http);
        if (response == null) {
            return error(500, "internal server error");
        }
        return render(response, "HEAD".equals(http.method()));
    }

    private MockHttpServletResponse dispatch(IlmekCallContext call, HttpCall http) {
        MockHttpServletRequestBuilder builder = MockMvcRequestBuilders
                .request(HttpMethod.valueOf(http.method()), http.uri());
        http.query().forEach((name, values) -> builder.queryParam(name, values.toArray(String[]::new)));
        http.headers().forEach((name, value) -> {
            if (!DROPPED_HEADERS.contains(name)) {
                builder.header(name, value);
            }
        });
        if (http.body() != null) {
            builder.contentType(MediaType.APPLICATION_JSON).content(http.body());
        }
        if (call.principalId() != null) {
            builder.header(PRINCIPAL_HEADER, call.principalId());
        }
        try {
            MvcResult result = mockMvc().perform(builder).andReturn();
            if (result.getRequest().isAsyncStarted()) {
                result.getAsyncResult();
                result = mockMvc().perform(asyncDispatch(result)).andReturn();
            }
            return result.getResponse();
        } catch (ServletException | RuntimeException e) {
            LOG.warn("controller failed for " + http.method() + " " + http.uri().getRawPath() + ": " + e);
            return null;
        } catch (Exception e) {
            throw new AppException(ErrorCode.INTERNAL, "request could not be executed", false);
        }
    }

    private byte[] render(MockHttpServletResponse response, boolean head) {
        byte[] raw = head ? new byte[0] : response.getContentAsByteArray();
        if (raw.length > MAX_PAYLOAD_BYTES) {
            return error(413, "payload too large");
        }
        ObjectNode result = HttpCall.MAPPER.createObjectNode().put("status", response.getStatus());
        ObjectNode headers = result.putObject("headers");
        for (String name : RETURNED_HEADERS) {
            List<String> values = response.getHeaders(name);
            if (!values.isEmpty()) {
                headers.put(name.toLowerCase(Locale.ROOT), String.join(", ", values));
            }
        }
        if (raw.length > 0) {
            MediaType type = contentType(response);
            if (type != null && isJson(type)) {
                result.set("body", parseJson(raw));
            } else if (type != null && "text".equals(type.getType())) {
                result.put("text", new String(raw, charset(type)));
            } else {
                return error(415, "binary responses are not supported");
            }
        }
        return serialize(result);
    }

    private static MediaType contentType(MockHttpServletResponse response) {
        String header = response.getContentType();
        if (header == null) {
            return null;
        }
        try {
            return MediaType.parseMediaType(header);
        } catch (RuntimeException e) {
            return null;
        }
    }

    private static boolean isJson(MediaType type) {
        return "application".equals(type.getType())
                && ("json".equals(type.getSubtype()) || type.getSubtype().endsWith("+json"));
    }

    private static Charset charset(MediaType type) {
        Charset charset = type.getCharset();
        return charset != null ? charset : StandardCharsets.UTF_8;
    }

    private static JsonNode parseJson(byte[] raw) {
        try {
            return HttpCall.MAPPER.readTree(raw);
        } catch (IOException e) {
            throw new AppException(ErrorCode.INTERNAL, "controller returned invalid JSON", false);
        }
    }

    private static byte[] error(int status, String message) {
        ObjectNode result = HttpCall.MAPPER.createObjectNode().put("status", status);
        result.putObject("headers");
        result.putObject("body").put("error", message);
        return serialize(result);
    }

    private static byte[] serialize(ObjectNode result) {
        try {
            byte[] bytes = HttpCall.MAPPER.writeValueAsBytes(result);
            return bytes.length > MAX_PAYLOAD_BYTES ? error(413, "payload too large") : bytes;
        } catch (IOException e) {
            throw new AppException(ErrorCode.INTERNAL, "response could not be serialized", false);
        }
    }

    private MockMvc mockMvc() {
        MockMvc current = mockMvc;
        if (current == null) {
            synchronized (this) {
                current = mockMvc;
                if (current == null) {
                    current = MockMvcBuilders.webAppContextSetup(context).addFilters(filters()).build();
                    mockMvc = current;
                }
            }
        }
        return current;
    }

    private Filter[] filters() {
        List<Object> candidates = new ArrayList<>(context.getBeansOfType(Filter.class).values());
        candidates.addAll(context.getBeansOfType(FilterRegistrationBean.class).values());
        AnnotationAwareOrderComparator.sort(candidates);
        List<Filter> filters = new ArrayList<>();
        for (Object candidate : candidates) {
            Filter filter = candidate instanceof FilterRegistrationBean<?> registration
                    ? registration.getFilter() : (Filter) candidate;
            if (filter != null && filters.stream().noneMatch(known -> known == filter)) {
                filters.add(filter);
            }
        }
        return filters.toArray(Filter[]::new);
    }
}
