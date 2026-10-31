package dev.ilmek.sdk.store;

import org.springframework.data.jpa.repository.JpaRepository;

public interface IlmekOperationRepository extends JpaRepository<IlmekOperation, IlmekOperation.Key> {
}
