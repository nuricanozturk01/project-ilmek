package dev.ilmek.sdk.store;

import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.wire.ErrorCode;

public class CursorExpiredException extends AppException {

    public CursorExpiredException() {
        super(ErrorCode.CURSOR_EXPIRED, "cursor precedes the retained event window", false);
    }
}
