# İlmek Spring web adapter

Runs an existing Spring MVC `@RestController` application on İlmek without changing its controllers. The adapter registers two İlmek methods, `http.read` and `http.write`, and turns each İlmek call into an in-process `MockMvc` request against the application's own `WebApplicationContext`. No HTTP listener is opened.

## Dependency

Artifact `dev.ilmek:ilmek-spring-web-adapter` (Gradle project `web-adapter`), next to the starter in this build. It depends on `dev.ilmek:starter` and needs the starter's usual consumer wiring (see `../README.md`).

```kotlin
implementation("dev.ilmek:starter")
implementation("dev.ilmek:ilmek-spring-web-adapter")
```

`spring-webmvc` and `spring-test` are runtime dependencies on purpose: `MockMvc` is used at runtime. The application must be a servlet web application (for example `spring-boot-starter-web` on Boot 3.5, `spring-boot-starter-webmvc` on Boot 4.1).

## Zero-code usage

Add the dependency. Auto-configuration does the rest:

- registers `http.read` (READ) and `http.write` (WRITE);
- sets `server.port=-1` (no HTTP endpoints, `WebApplicationContext` kept) when the application did not set `server.port`. Set it yourself to keep a real listener.

Writes go through the starter's operation store, so a retried `http.write` with the same operation id and body replays the stored result and does not call the controller again.

## Wire contract

Request body (JSON object; unknown or duplicate fields are rejected):

| Field | Type | Notes |
| --- | --- | --- |
| `method` | string | `GET`, `HEAD`, `OPTIONS` for `http.read`; `POST`, `PUT`, `PATCH`, `DELETE` for `http.write` |
| `path` | string | Starts with `/`; no scheme, host, query, fragment, backslash, space or control character |
| `query` | object, optional | `{"name": ["v1", "v2"]}`; repeated values are repeated query parameters |
| `headers` | object, optional | `{"lowercase-name": "value"}` |
| `body` | any JSON value, optional | `http.write` only; sent as `application/json` |

Result body: `{"status": 200, "headers": {...}, "body": <json>}` or `"text": "<string>"` instead of `body`.

- `headers` holds only `content-type`, `location`, `etag`, `cache-control`.
- `body` is present for `application/json` and `+json` responses; `text` for `text/*` responses; neither for empty responses.
- `HEAD` results never carry a body.
- Controller outcomes are results, not İlmek errors: 400, 404, 405 and 500 arrive as `status`. An uncaught controller exception is `500` with body `{"error":"internal server error"}`.
- A request outside the contract (bad path, method not allowed for the kind, `body` on `http.read`, malformed JSON) fails with `INVALID_MESSAGE`.
- Binary responses (anything that is not JSON or `text/*`): `415` with `{"error":"binary responses are not supported"}`.
- A request or response over 200 KiB: `413` with `{"error":"payload too large"}`.

Request headers `cookie`, `authorization`, `host` and `content-length` are dropped before the controller sees them.

## Principal

The adapter sets the request header `X-Ilmek-Principal` to the host-supplied principal id. Any client-supplied `x-ilmek-principal` is removed first. The value is NOT authenticated: it is whatever the host passes in the call, and it must not be used as a security boundary.

## Limitations

- Servlet applications only. WebFlux (reactive) is not supported; the auto-configuration backs off.
- Spring Security is not supported in this version (no login, sessions, CSRF). If it is on the classpath, one WARN is logged at startup.
- Multipart and form requests are not supported; neither are binary responses.
- Error pages are not dispatched: a `sendError` or unmapped path yields the status with no body.
- Application filters (`Filter` beans and `FilterRegistrationBean` filters) are applied; the `DispatcherServlet` is a fresh one built by `MockMvc`, so `spring.mvc.*` dispatcher settings are not applied.
- A controller's own transactions join the starter's write transaction only when they use the same transaction manager. This is the design intent and is not covered by a test here.
- A 4xx/5xx result of `http.write` is stored by the operation store and replayed on retry, like any other result.
- Async return types (`Callable`, `DeferredResult`) are completed before replying; streaming responses are not supported.
- `MockMvc` runs in the application's process, so responses and requests are buffered in memory (bounded by the 200 KiB limits above).
- The adapter registers its `EnvironmentPostProcessor` under the pre-4.0 interface key in `META-INF/spring.factories`. Boot 4.1 still honours it, but the interface is deprecated for removal.

## Tests

```
./gradlew -PbootVersion=3.5.16 :web-adapter:test
./gradlew -PbootVersion=4.1.2 :web-adapter:test
```

The test sources contain a small `@RestController` application and drive the registered handlers, including a retried write over the real backend IPC client and a check that no port is listening.
