# Spring Boot + Angular applications

An İlmek application can use a Spring Boot backend and an Angular frontend. The backend runs as a
child process of the host and talks to it over loopback IPC; it opens no network port. Two styles
exist:

| Style | Backend code | Frontend code | Example |
| --- | --- | --- | --- |
| **Native methods** | methods annotated with `@IlmekMethod` | `window.ilmek.call` / `subscribe` | `examples/tasks` |
| **Unchanged REST app** | ordinary `@RestController`s | ordinary `HttpClient` calls to `/api/...` | `examples/rest-notes` |

Libraries: the backend starter [`java/ilmek-spring-boot-starter`](../java/ilmek-spring-boot-starter/README.md)
(IPC client, method registry, idempotent write store, event store), its
[web adapter](../java/ilmek-spring-boot-starter/web-adapter/README.md) (runs Spring MVC controllers
on İlmek) and the npm package [`@ilmek/angular-http`](../packages/angular-http/README.md) (routes
`HttpClient` calls over İlmek). The frontend never imports the SDK: the client injects `window.ilmek`.

## Supported versions

All combinations below were run (Linux x86_64, Gradle 9.4.1). Java is compiled with `--release 17`.

| Component | Supported | Default | Notes |
| --- | --- | --- | --- |
| Java | 17, 21, 25 | — | 17 is the baseline |
| Spring Boot | 3.5.x (tested 3.5.16), 4.1.x (tested 4.1.2) | 4.1.2 | select with `-PbootVersion=…` |
| Angular | 22.2.4 (in the repository), 21.2.x (build tested) | 22.2.4 | the frontend only uses `window.ilmek` and static files |

The starter uses its own Jackson 2 (2.22.4), so Boot 4's Jackson 3 does not interfere. Boot 4.2 was
only a release candidate and is not targeted; Angular 20 was not tried.

## Publish an application

Layout:

```
my-app/
  app/app.toml          # manifest (`ilmek app init` writes a template)
  frontend-angular/
  backend-java/
```

1. Manifest: `ilmek app init --namespace <ns> --slug <name> /tmp/scaffold`, copy the generated
   `app.toml` to `my-app/app/app.toml`. Never change or delete `app_id` and `publisher_id`. Set
   `backend_required = true`; `permissions` must contain `app.call` (and `app.subscribe` for live
   updates).
2. Install frontend dependencies: `cd frontend-angular && npm ci`.
3. Build the backend: `cd backend-java && ./gradlew bootJar`.
4. Package: `scripts/packaging/pack-angular-app.sh frontend-angular my-app /tmp/pkg`. It runs
   `ng build --configuration production`, replaces `my-app/app/assets/` with the single
   `dist/*/browser` output (source maps excluded) and calls `ilmek app pack`. `/tmp/pkg` must not
   exist and `ilmek` must be on `PATH`.
5. With the node running (same `ILMEK_HOME`):

   ```sh
   ilmek app install /tmp/pkg
   ilmek app set-backend <ns>/<name> --exe "$(command -v java)" --arg=-jar --arg=<path-to-jar>
   ilmek app start <ns>/<name>
   ilmek app publish <ns>/<name>
   ```

6. Open `ilmek://<ns>/<name>` in a client.

To release a new version: raise `version` in `app.toml`, package again, `ilmek app update /tmp/pkg2`,
then `ilmek app publish <ns>/<name>`.

Frontend rules (the content policy enforces them): no service worker, no external URLs, no inline
`<style>`/`<script>` (set `optimization.styles.inlineCritical` and `fonts` to `false` in
`angular.json`, as the examples do). Running `ng serve` (`localhost:4200`) shows the page as an
ordinary web page without `window.ilmek`; use it only for layout work.

## Native methods (`examples/tasks`)

Do not add `spring-boot-starter-web` to a native-methods backend: it would open a Tomcat port
outside İlmek. Methods are annotated instead:

```java
@IlmekMethod(name = "tasks.create", kind = MethodKind.WRITE)
public byte[] create(IlmekCallContext context, byte[] body) { … }
```

Results must be JSON objects. Writes are idempotent through the starter's operation store. The
example exposes `tasks.list`, `tasks.create`, `tasks.complete`, `tasks.update`, `tasks.delete`,
`tasks.clear_completed` and the `tasks.events` topic; its data is an H2 file with `WRITE_DELAY=0`
so committed rows survive a `kill -9`. See [`examples/tasks/backend-java/README.md`](../examples/tasks/backend-java/README.md)
and [`examples/tasks/frontend-angular/README.md`](../examples/tasks/frontend-angular/README.md).
Test a backend without a UI: `ilmek call ilmek://nuri/tasks tasks.list`.

## Moving an existing REST app (`examples/rest-notes`)

A Spring MVC REST app plus an Angular `HttpClient` frontend runs on İlmek with its controllers and
components unchanged. What is added:

| | Unchanged | Added |
| --- | --- | --- |
| Backend | `@RestController`, bean validation, JPA, repositories | Gradle dependencies `dev.ilmek:starter` and `dev.ilmek:ilmek-spring-web-adapter`; one `@AutoConfigurationPackage(basePackages = {"<your.package>", "dev.ilmek.sdk.store"})` on the main class |
| Frontend | components, `HttpClient` calls to `/api/...` | `@ilmek/angular-http` dependency, `provideIlmekHttp()` in `app.config.ts`, `install-links=true` in `.npmrc` |
| Package | | `app/app.toml` |

How it works: a call to `/api/...` becomes `window.ilmek.call("http.read" | "http.write", …)`.
`GET/HEAD/OPTIONS` are reads; `POST/PUT/PATCH/DELETE` are writes. In the backend the adapter hands
the request to the application's own `DispatcherServlet` through `MockMvc`. A write retried with
the same operation id does not run the controller twice. The adapter sets `server.port=-1`, so the
application listens on no port (the e2e test checks that the jar holds no listening TCP socket).
Details and the wire contract: the [web adapter README](../java/ilmek-spring-boot-starter/web-adapter/README.md).

Keep the application's own package in the `@AutoConfigurationPackage` list, otherwise your entities
are not found. `install-links=true` is needed because with a symlinked `file:` dependency the package
resolves its own Angular copy and `HttpClient` silently ignores the interceptor.

Not supported: Spring Security (login, sessions, CSRF; a warning is logged), WebFlux, multipart
uploads and binary responses, WebSocket/SSE (use İlmek `subscribe`), cookies and `Authorization`
headers (dropped), external URLs, requests or responses over 200 KiB (HTTP 413). The
`X-Ilmek-Principal` header is the declared device id, not an authenticated user; do not use it for
security decisions. An unexpected controller error (500) is stored as the result of that operation
id and replayed on a retry. Whether the controller's own transactions join the starter's write
transaction (same transaction manager) is the design intent and is not covered by a test.

## Test status

| Check | Result |
| --- | --- |
| Starter tests (Java 17/21/25 × Boot 3.5.16/4.1.2) | 131 tests, all cells passed |
| Web adapter tests (Java 17/21/25 × Boot 3.5.16/4.1.2) | 33 tests, all cells passed |
| `examples/tasks` backend tests | 22 passed (Boot 3.5.16 and 4.1.2) |
| `examples/rest-notes` backend tests | 11 passed (Boot 3.5.16 and 4.1.2) |
| Real host + real jar, native methods | `cargo test -p ilmek-testing --test java_backend_e2e`: 6 passed |
| Real host + real jar, REST adapter | `cargo test -p ilmek-testing --test rest_notes_e2e`: 3 passed |
| `tasks` frontend tests / `rest-notes` frontend tests | 23 / 11 passed |
| Angular 21.2 build and pack | passed |
| `pack-angular-app.sh` with the real `ng` (`rest-notes`: "Packed 4 files") | passed |
| Install, `set-backend`, `start`, `publish` of the Java examples on a node | not run |
| Rendering of the Java/Angular examples in the desktop client | not observed (an earlier attempt produced no screenshot) |

The ViewEncapsulation question (component styles injected at runtime may be blocked by the content
policy) is unresolved for `tasks`; the fix, if needed, is `ViewEncapsulation.None` with the global
`src/styles.css`, not a policy change.
