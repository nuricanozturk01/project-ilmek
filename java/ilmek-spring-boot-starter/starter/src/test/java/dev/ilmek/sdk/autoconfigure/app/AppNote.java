package dev.ilmek.sdk.autoconfigure.app;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

@Entity
@Table(name = "app_note")
public class AppNote {

    @Id
    private Long id;
}
