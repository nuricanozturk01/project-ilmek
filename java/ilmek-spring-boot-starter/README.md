# İlmek Spring Boot starter

The starter lets a Spring Boot application act as an İlmek backend. It provides:

- the IPC client that connects the backend to the local `ilmek-host` over loopback (`ILMEK/1` frames inside the host's IPC link);
- the `@IlmekMethod` method registry (`dev.ilmek.sdk.api`);
- a JPA-backed operation store (idempotent writes) and event store (topic events with cursors and retention).

It is not a web framework: the starter opens no listening socket and exposes no HTTP; it connects to the loopback IPC port that the host supplies.

## Dependency

The starter is built from this repository as `dev.ilmek:starter:0.1.0-SNAPSHOT`. The example consumer includes it as a composite build in `examples/tasks/backend-java/settings.gradle.kts`:

```kotlin
rootProject.name = "backend-java"

includeBuild("../../../java/ilmek-spring-boot-starter")
```

and in its `build.gradle.kts`:

```kotlin
implementation("dev.ilmek:starter")
```

## Required consumer wiring

Put these annotations on the application class (the example uses `TasksApplication`):

```java
@SpringBootApplication
@AutoConfigurationPackage(basePackageClasses = {TasksApplication.class, IlmekOperation.class})
@EnableJpaRepositories(basePackageClasses = {TasksApplication.class, IlmekOperationRepository.class})
public class TasksApplication {
    // ...
}
```

`IlmekOperation.class` and `IlmekOperationRepository.class` are in `dev.ilmek.sdk.store`, so scanning that package registers the operation, event, and event-floor entities and repositories together.

`@AutoConfigurationPackage` is used instead of `@EntityScan` because it has the same package on Spring Boot 3.5 and 4.1 (`@EntityScan` moved in Boot 4). Keep the application's own package in `basePackageClasses`: an explicit `@AutoConfigurationPackage` replaces the one that `@SpringBootApplication` adds.

Without the `@AutoConfigurationPackage` and `@EnableJpaRepositories` lines, the starter store beans (`IlmekOperationsService`, `IlmekEventsService`) are silently absent. The starter activates only when all three repositories are registered, and the application has no error to show for it. Starting the application without these annotations therefore starts no IPC client either.

The consumer must also provide a data source. The example uses H2 with `WRITE_DELAY=0` so that committed rows survive a `kill -9`.

## Startup behavior

The IPC client starts once, on `ApplicationReadyEvent`, on a non-daemon thread. It reads one JSON configuration object from standard input (the host writes it; `data_dir`, `app_id`, `community_id`, `ipc_port`, `token`).

| Standard input | Behavior | Process exit |
| --- | --- | --- |
| Empty, or nothing before 10 s | Logged at INFO; no client is started. Normal Spring applications are unaffected. | Not stopped |
| Not valid configuration (unparseable, unknown field, missing field) | Logged at WARN; the context is stopped | 1 |
| Parsed but rejected by the client (for example, bad port) or any other client failure | Logged at ERROR; the context is stopped | 1 |
| Valid | Connects to the loopback IPC port, completes the `IPC_READY` handshake, and serves until the host closes the link | 0 when the host closes the link; 1 if the handshake or link fails |

The client stops the application through `SpringApplication.exit(context)` and then calls the process exit hook (`IntConsumer` bean `ilmekProcessExit`, default `System::exit`). Tests replace that bean to observe the status.

## Identity

The principal id on each call is supplied by the host. The starter does not authenticate it; the host and the IPC link's token are the trust boundary. Application code must treat `IlmekCallContext.principalId()` as host-provided and must not use it as an authorization decision on its own.

## Writing methods

Annotate methods with `@IlmekMethod(name = "...", kind = MethodKind.READ | WRITE)`. The method signature is `byte[] method(IlmekCallContext context, byte[] body)`. Results must be JSON objects of at most 262,144 bytes; other results become `INTERNAL`. Write methods run through `IlmekOperationsService.write`, which is transactional and keyed by the operation id, so a retry with the same id replays the stored result.

Events are appended with `IlmekEventsService.append(topic, body)`. The body must be a JSON object of at most 262,144 bytes; other bodies throw `IllegalArgumentException` before anything is stored. Subscribers read in pages of `IlmekEventsService.PAGE_SIZE` (16) rows, refilled by acknowledgement. Each topic keeps 10,000 events; an older cursor is answered with `CURSOR_EXPIRED`.

## Supported versions

| Item | Supported | Default |
| --- | --- | --- |
| Java | 17, 21, 25 (compiled with `--release 17`) | any of them |
| Spring Boot | 3.5.x (tested 3.5.16), 4.1.x (tested 4.1.2) | 4.1.2 |

Select the Boot line with `-PbootVersion`. The starter uses its own Jackson 2 (`com.fasterxml.jackson`, pinned to 2.22.4), so Boot 4's Jackson 3 does not affect it. Only the two test helpers `IlmekJpaTest` and `JpaAutoConfigurations` differ per line (`src/test-boot3`, `src/test-boot4`); production code is shared.

## Tests

```sh
cd java/ilmek-spring-boot-starter
./gradlew :starter:test                         # Boot 4.1.2
./gradlew -PbootVersion=3.5.16 :starter:test    # Boot 3.5
./gradlew -Dorg.gradle.java.home=/path/to/jdk :starter:test   # choose the JDK
```

The suite includes a real loopback `ServerSocket` acting as the host for handshake, link-close exit, once-only startup, and absent or invalid configuration.

## Example

`examples/tasks/backend-java` is a complete consumer: build and run instructions are in [examples/tasks/backend-java/README.md](../../examples/tasks/backend-java/README.md).
