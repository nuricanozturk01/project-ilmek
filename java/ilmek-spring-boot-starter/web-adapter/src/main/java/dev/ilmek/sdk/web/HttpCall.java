package dev.ilmek.sdk.web;

import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.JsonNodeType;
import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.wire.ErrorCode;
import java.io.IOException;
import java.net.URI;
import java.net.URISyntaxException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

record HttpCall(String method, URI uri, Map<String, List<String>> query, Map<String, String> headers,
                byte[] body) {

    static final ObjectMapper MAPPER = new ObjectMapper()
            .enable(JsonParser.Feature.STRICT_DUPLICATE_DETECTION)
            .enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS);

    private static final Set<String> FIELDS = Set.of("method", "path", "query", "headers", "body");
    private static final Pattern HEADER_NAME = Pattern.compile("[A-Za-z0-9!#$%&'*+.^_`|~-]+");
    private static final Pattern FORBIDDEN_CHARS = Pattern.compile("[\\x00-\\x20\\x7f\\\\?#]");

    static HttpCall parse(byte[] request, Set<String> allowedMethods, boolean bodyAllowed) {
        JsonNode root;
        try {
            root = MAPPER.readTree(request);
        } catch (IOException e) {
            throw invalid("request is not valid JSON");
        }
        if (root == null || root.getNodeType() != JsonNodeType.OBJECT) {
            throw invalid("request must be a JSON object");
        }
        root.fieldNames().forEachRemaining(name -> {
            if (!FIELDS.contains(name)) {
                throw invalid("unknown request field");
            }
        });
        String method = text(root.get("method"), "method");
        if (!allowedMethods.contains(method)) {
            throw invalid("method not allowed for this call");
        }
        URI uri = uri(text(root.get("path"), "path"));
        if (root.has("body") && !bodyAllowed) {
            throw invalid("this call must not carry a body");
        }
        byte[] body = null;
        if (root.has("body")) {
            try {
                body = MAPPER.writeValueAsBytes(root.get("body"));
            } catch (IOException e) {
                throw invalid("body cannot be serialized");
            }
        }
        return new HttpCall(method, uri, query(root.get("query")), headers(root.get("headers")), body);
    }

    private static URI uri(String path) {
        if (!path.startsWith("/") || path.startsWith("//") || FORBIDDEN_CHARS.matcher(path).find()) {
            throw invalid("path must be an absolute path without query or fragment");
        }
        try {
            return new URI(path);
        } catch (URISyntaxException e) {
            throw invalid("path is not a valid URI path");
        }
    }

    private static Map<String, List<String>> query(JsonNode node) {
        Map<String, List<String>> query = new LinkedHashMap<>();
        if (node == null) {
            return query;
        }
        if (node.getNodeType() != JsonNodeType.OBJECT) {
            throw invalid("query must be an object");
        }
        node.properties().forEach(entry -> {
            if (entry.getKey().isEmpty() || entry.getValue().getNodeType() != JsonNodeType.ARRAY) {
                throw invalid("query values must be arrays of strings");
            }
            List<String> values = new ArrayList<>();
            entry.getValue().forEach(value -> values.add(text(value, "query value")));
            query.put(entry.getKey(), values);
        });
        return query;
    }

    private static Map<String, String> headers(JsonNode node) {
        Map<String, String> headers = new LinkedHashMap<>();
        if (node == null) {
            return headers;
        }
        if (node.getNodeType() != JsonNodeType.OBJECT) {
            throw invalid("headers must be an object");
        }
        node.properties().forEach(entry -> {
            String value = text(entry.getValue(), "header value");
            if (!HEADER_NAME.matcher(entry.getKey()).matches() || value.indexOf('\r') >= 0 || value.indexOf('\n') >= 0) {
                throw invalid("invalid header");
            }
            headers.put(entry.getKey().toLowerCase(Locale.ROOT), value);
        });
        return headers;
    }

    private static String text(JsonNode node, String what) {
        if (node == null || node.getNodeType() != JsonNodeType.STRING) {
            throw invalid(what + " must be a string");
        }
        return node.asText();
    }

    private static AppException invalid(String message) {
        return new AppException(ErrorCode.INVALID_MESSAGE, message, false);
    }
}
