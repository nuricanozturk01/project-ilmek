# Tasks backend (Spring Boot)

Spring Boot backend for the İlmek todo example. It runs as a child process of the host (`java -jar backend-java.jar`) and exposes `tasks.list`, `tasks.create`, `tasks.complete` and the `tasks.events` topic through the Java starter.

## Build

```sh
./gradlew bootJar
```

The jar is written to `build/libs/backend-java.jar`.

## Durability

`src/main/resources/application.properties` sets `WRITE_DELAY=0` on the H2 file URL. The H2 default delays persisting commits, so a committed write (and its operation record) was lost when the process was killed. This was reproduced with a standalone `kill -9` experiment and with the real-host test.

## Verification

Environment (this run): Linux x86_64, `openjdk version "25.0.4.1.1"`, Gradle wrapper 9.4.1 (`gradle-wrapper.properties`), commit `1c14b6c` for the backend, real `ilmek-host` built from this repository. All checks are real-process runs; loopback only (no Wi-Fi).

Commands and observed results from this run:

| Check | Status | Command | Observed |
| --- | --- | --- | --- |
| Backend unit and integration tests | PASS | `cd examples/tasks/backend-java && ./gradlew test` | `BUILD SUCCESSFUL`; TaskServiceTest 21 tests, TasksApplicationTest 1 test, 0 failures |
| Backend jar | PASS | `cd examples/tasks/backend-java && ./gradlew bootJar` | `BUILD SUCCESSFUL`; `build/libs/backend-java.jar` written |
| R3-1 plain read (`tasks.list`) | PASS | `cargo test -p ilmek-testing --test java_backend_e2e -- --nocapture` | `plain_read_lists_tasks_from_the_real_backend ... ok` |
| R3-2 plain write (`tasks.create` then list) | PASS | same | `plain_write_creates_a_task_that_the_next_read_shows ... ok` |
| R3-3 identical retry replays one row | PASS | same | `identical_retry_returns_the_same_result_and_writes_one_row ... ok` |
| R3-4 conflicting retry gives `OPERATION_CONFLICT` | PASS | same | `reusing_an_operation_id_with_a_different_body_is_a_conflict ... ok` |
| R3-5 subscribe receives the flat `created` event | PASS | same | `a_subscriber_receives_the_event_after_a_create ... ok` |
| R3-6 SIGKILL backend, host restart, replay same operation id | PASS | same | `a_backend_kill_then_host_restart_keeps_the_write_and_replays_it_once ... ok` |
| Suite summary (at HEAD `936e688`) | PASS | same | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 24.52s` |

Earlier (not re-run in this pass): before the durability fix, R3-6 failed under the parallel suite because the replay created a new row instead of returning the stored result. A standalone H2 `kill -9` after commit showed the default URL lost the row and `WRITE_DELAY=0` kept it.

Not verified: the Tauri GUI rendering of the Angular app (see `examples/tasks/frontend-angular/README.md`), and any physical Wi-Fi or multi-host run.
