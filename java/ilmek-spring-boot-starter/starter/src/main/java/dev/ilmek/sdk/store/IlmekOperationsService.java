package dev.ilmek.sdk.store;

import java.util.function.Supplier;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class IlmekOperationsService {

    private final IlmekOperationRepository repository;

    public IlmekOperationsService(IlmekOperationRepository repository) {
        this.repository = repository;
    }

    @Transactional
    public byte[] write(String principalId, String operationId, String method, String payloadHash,
                        Supplier<byte[]> businessLogic) {
        IlmekOperation.Key key = new IlmekOperation.Key(principalId, operationId);
        var stored = repository.findById(key);
        if (stored.isPresent()) {
            IlmekOperation operation = stored.get();
            if (!operation.getMethod().equals(method) || !operation.getPayloadHash().equals(payloadHash)) {
                throw new OperationConflictException();
            }
            return operation.getResult();
        }
        byte[] result = businessLogic.get();
        repository.save(new IlmekOperation(principalId, operationId, method, payloadHash, result,
            System.currentTimeMillis()));
        return result;
    }
}
