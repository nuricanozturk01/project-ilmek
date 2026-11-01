# Tasks backend (Spring Boot)

Spring Boot backend of the İlmek todo example. The host runs it as a child process
(`java -jar backend-java.jar`); it exposes `tasks.list`, `tasks.create`, `tasks.complete`,
`tasks.update`, `tasks.delete`, `tasks.clear_completed` and the `tasks.events` topic through the
Java starter. Overview, versions and publishing steps: [JAVA_ANGULAR](../../../docs/JAVA_ANGULAR.md).

## Build and test

```sh
./gradlew bootJar     # build/libs/backend-java.jar
./gradlew test        # 22 tests; -PbootVersion=3.5.16 selects Spring Boot 3.5
```

## Methods

| Method | Kind | Body | Reply |
| --- | --- | --- | --- |
| `tasks.list` | read | `{}` | `{"tasks":[{id,title,done}]}` |
| `tasks.create` | write | `{"title"}` | `{"task":{...}}` |
| `tasks.complete` | write | `{"id","done"?}` | `{"task":{...}}` (`done` defaults to true) |
| `tasks.update` | write | `{"id","title"}` | `{"task":{...}}` |
| `tasks.delete` | write | `{"id"}` | `{"deleted":id}` |
| `tasks.clear_completed` | write | `{}` | `{"removed":n}` |

Every reply is a JSON object (the host rejects arrays). Every change is appended to `tasks.events`
as a flat object `{"type","id",...}`. Titles are validated by hand (1–200 characters); errors use
`AppException` and `ErrorCode`. Test without a UI: `ilmek call ilmek://nuri/tasks tasks.list`.

## Durability

`application.properties` sets `WRITE_DELAY=0` on the H2 file URL. H2's default delays persisting
commits, so a committed write and its operation record were lost when the process was killed
(reproduced with a standalone `kill -9` experiment and with the real-host test).

## Verification

Environment: Linux x86_64, OpenJDK 25.0.4.1.1, Gradle 9.4.1, loopback only.

| Check | Status | Command |
| --- | --- | --- |
| Unit and integration tests | PASS (22) | `./gradlew test` |
| Real host + real jar: read, write, identical retry replays one row, conflicting retry gives `OPERATION_CONFLICT`, subscription event, `SIGKILL` of the backend then replay | PASS (6) | `cargo test -p ilmek-testing --test java_backend_e2e` |
| Install on a node and view in the desktop client | NOT RUN | |
