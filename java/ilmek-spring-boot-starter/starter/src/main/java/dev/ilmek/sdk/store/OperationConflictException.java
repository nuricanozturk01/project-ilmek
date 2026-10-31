package dev.ilmek.sdk.store;

import dev.ilmek.sdk.api.AppException;
import dev.ilmek.sdk.wire.ErrorCode;

public class OperationConflictException extends AppException {

    public OperationConflictException() {
        super(ErrorCode.OPERATION_CONFLICT, "operation id reused with different content", false);
    }
}
