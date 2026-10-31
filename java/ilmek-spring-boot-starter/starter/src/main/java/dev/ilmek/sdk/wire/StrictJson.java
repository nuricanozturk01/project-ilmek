package dev.ilmek.sdk.wire;

import com.fasterxml.jackson.core.JacksonException;
import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.core.JsonToken;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

public final class StrictJson {

    private static final byte[] BOM = {(byte) 0xEF, (byte) 0xBB, (byte) 0xBF};
    private static final Set<String> STRING_FIELDS = Set.of("type", "request_id", "community_id");
    private static final Set<String> FIELDS = Set.of("type", "request_id", "community_id", "data");
    private static final ObjectMapper MAPPER = new ObjectMapper()
            .enable(DeserializationFeature.FAIL_ON_READING_DUP_TREE_KEY);

    private StrictJson() {
    }

    public static Envelope parseEnvelope(byte[] metadataJson) {
        if (startsWithBom(metadataJson)) {
            throw new StrictJsonException("byte order mark is not allowed");
        }
        String text = decodeUtf8(metadataJson);
        try (JsonParser parser = MAPPER.getFactory().createParser(text)) {
            if (parser.nextToken() != JsonToken.START_OBJECT) {
                throw new StrictJsonException("top-level value must be an object");
            }
            Set<String> seen = new HashSet<>();
            Map<String, String> strings = new HashMap<>();
            JsonNode data = null;
            JsonToken token;
            while ((token = parser.nextToken()) == JsonToken.FIELD_NAME) {
                String name = parser.currentName();
                if (!seen.add(name)) {
                    throw new StrictJsonException("duplicate object key `" + name + "`");
                }
                if (!FIELDS.contains(name)) {
                    throw new StrictJsonException("unknown top-level field `" + name + "`");
                }
                parser.nextToken();
                if (STRING_FIELDS.contains(name)) {
                    if (parser.currentToken() != JsonToken.VALUE_STRING) {
                        throw new StrictJsonException("`" + name + "` must be a string");
                    }
                    strings.put(name, parser.getText());
                } else {
                    if (parser.currentToken() != JsonToken.START_OBJECT) {
                        throw new StrictJsonException("`data` must be an object");
                    }
                    data = MAPPER.readTree(parser);
                }
            }
            if (token != JsonToken.END_OBJECT) {
                throw new StrictJsonException("expected end of top-level object");
            }
            if (parser.nextToken() != null) {
                throw new StrictJsonException("trailing content after top-level object");
            }
            if (!seen.equals(FIELDS)) {
                Set<String> missing = new HashSet<>(FIELDS);
                missing.removeAll(seen);
                throw new StrictJsonException("missing top-level field(s): " + missing);
            }
            return new Envelope(strings.get("type"), strings.get("request_id"), strings.get("community_id"), data);
        } catch (JacksonException e) {
            throw new StrictJsonException("malformed JSON: " + e.getOriginalMessage(), e);
        } catch (java.io.IOException e) {
            throw new StrictJsonException("malformed JSON: " + e.getMessage(), e);
        }
    }

    private static boolean startsWithBom(byte[] bytes) {
        if (bytes.length < BOM.length) {
            return false;
        }
        for (int i = 0; i < BOM.length; i++) {
            if (bytes[i] != BOM[i]) {
                return false;
            }
        }
        return true;
    }

    private static String decodeUtf8(byte[] bytes) {
        try {
            return StandardCharsets.UTF_8.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes))
                    .toString();
        } catch (CharacterCodingException e) {
            throw new StrictJsonException("invalid UTF-8", e);
        }
    }
}
