# rest-notes

An ordinary Spring Boot REST controller and an ordinary Angular `HttpClient` app, running on İlmek with only dependencies and one provider added. Notes: list (with `?q=`), get, create, replace, delete over `/api/notes`.

Application address: `ilmek://nuri/notes-rest`.

## What stays untouched and what was added

| Part | Untouched (ordinary code) | Added for İlmek |
| --- | --- | --- |
| Backend (`backend-java`) | `@RestController` on `/api/notes`, bean validation, `@RestControllerAdvice`, JPA `Note` entity, Spring Data repository, `spring-boot-starter-webmvc`, H2 file DB. Sources contain no İlmek import and no `@IlmekMethod`. | Gradle: `dev.ilmek:starter`, `dev.ilmek:ilmek-spring-web-adapter` and the `includeBuild` of the starter (with a substitution for the adapter artifact). Source: one `@AutoConfigurationPackage(basePackages = {...})` line listing the app package and `dev.ilmek.sdk.store`. |
| Frontend (`frontend-angular`) | One component using `HttpClient` with relative `/api/notes` URLs. No `window.ilmek` in the app code. | `provideIlmekHttp()` in `app.config.ts`, the `@ilmek/angular-http` `file:` dependency, and `install-links=true` in `.npmrc` (see below). |
| Package (`app/app.toml`) | Manifest fields as for any app. | `backend_required = true`, `permissions = ["app.call"]`. |

The adapter sets `server.port=-1` itself; the app has no such property. The starter's `@EnableJpaRepositories` line was not needed here: with the `@AutoConfigurationPackage` line alone the starter's beans were present (the tests assert `http.read` and `http.write` are registered), because Spring Data scans the auto-configuration packages.

`.npmrc` `install-links=true` is required. With npm's default symlink for a `file:` directory, `@ilmek/angular-http` resolves its own `@angular/common` and `@angular/core` (its devDependencies) instead of the app's, so `provideHttpClient` registers the interceptor in a second Angular instance and `HttpClient` never uses it (observed: the first run of the interceptor test fell through to the fetch backend, `Failed to parse URL from /api/notes`). Copying the package instead of linking it fixed that. Re-run `npm install` after rebuilding `packages/angular-http`.

## Build

```sh
(cd packages/angular-http && bun run build)
(cd examples/rest-notes/backend-java && ./gradlew bootJar)      # build/libs/rest-notes-backend.jar
(cd examples/rest-notes/frontend-angular && npm install && npm run build)
```

## Publish on a running node

`<package-dir>` must not exist yet. `ilmek` must be on `PATH` and the node must be running with the same `ILMEK_HOME`.

```sh
scripts/packaging/pack-angular-app.sh examples/rest-notes/frontend-angular examples/rest-notes <package-dir>
ilmek app install <package-dir>
ilmek app set-backend nuri/notes-rest --exe "$(command -v java)" --arg=-jar --arg="$PWD/examples/rest-notes/backend-java/build/libs/rest-notes-backend.jar"
ilmek app start nuri/notes-rest
ilmek app publish nuri/notes-rest
```

Only the `pack` step was run; the other four commands were not run against a node.

## Standalone use

Without the İlmek host the jar starts normally (with stdin at EOF it logs `No ILMEK backend configuration on standard input; backend client not started` at once). It does NOT serve HTTP by default, because the adapter sets `server.port=-1` (observed: `Tomcat started on port -1`, `curl localhost:8080` refused). To serve HTTP, set a port explicitly:

```sh
java -jar backend-java/build/libs/rest-notes-backend.jar --server.port=8080
```

Run that way, `GET /api/notes` answered 200 and an invalid POST answered 400 with the JSON error body.

## Verification

Linux x86_64, OpenJDK 25.0.4.1.1, Gradle 9.4.1, Node 24.17.0, Angular 22.2.4; loopback only.

| Check | Status | Command / observed |
| --- | --- | --- |
| Backend tests, Boot 4.1.2 and 3.5.16 | PASS | `cd backend-java && ./gradlew test` and `-PbootVersion=3.5.16`: 11 tests each |
| Backend sources contain no İlmek code | PASS | `grep -rn "Ilmek\|IlmekMethod\|import dev.ilmek" backend-java/src/main`: the SDK package appears only as a string in the `@AutoConfigurationPackage` line |
| Frontend tests | PASS | `cd frontend-angular && npm test`: 11 (8 component tests with `HttpTestingController`, 3 with the real `provideIlmekHttp` and a mocked `window.ilmek`); `GET /api/notes` became `ilmek.call("http.read", …)`, POST became `http.write` with `{write:true}` |
| Frontend production build | PASS | `index.html`: 0 `<style>`, 0 inline script bodies, 0 `<base>`, 0 `http(s)://` |
| Pack | PASS | `scripts/packaging/pack-angular-app.sh …`: `Packed 4 files` |
| Real host + real jar | PASS | `cargo test -p ilmek-testing --test rest_notes_e2e`: 3 tests (CRUD with 201/Location/400/404/204, same-operation-id retry gives one row, the jar holds no listening TCP socket) |
| Standalone HTTP with `--server.port=8080` | PASS | `GET /api/notes` 200; invalid POST 400 with the JSON error body |
| `install`/`set-backend`/`start`/`publish` on a node | NOT RUN | |
| Rendering in the desktop client | NOT RUN | |
