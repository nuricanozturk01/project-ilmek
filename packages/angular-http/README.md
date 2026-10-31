# @ilmek/angular-http

A functional `HttpInterceptorFn` that lets an existing Angular app keep using `HttpClient` while its `/api` calls travel over İlmek through `window.ilmek.call`. Requests that do not match the prefix pass through unchanged. Matched requests never fall back to a real network request.

## Install

Not published yet. Build it, then depend on the folder:

```sh
cd packages/angular-http && bun run build
# in the Angular app's package.json
"@ilmek/angular-http": "file:../../packages/angular-http"
```

Peer dependencies: `@angular/common` and `@angular/core` >=16, `rxjs` >=7.

## Setup

```ts
import { ApplicationConfig } from '@angular/core'
import { provideIlmekHttp } from '@ilmek/angular-http'

export const appConfig: ApplicationConfig = {
  providers: [provideIlmekHttp({ prefix: '/api' })],
}
```

`prefix` defaults to `/api`. `ilmekHttpInterceptor(options)` is also exported for use with your own `provideHttpClient(withInterceptors([...]))`.

## Supported

- GET, HEAD, OPTIONS, POST, PUT, PATCH, DELETE with JSON bodies and responses
- Query parameters (`HttpParams`, repeated keys), request and response headers
- `responseType` `json` and `text`
- Same-origin absolute URLs and relative URLs

## Not supported

- FormData, Blob, ArrayBuffer and typed-array request bodies; `blob` and `arraybuffer` response types (rejected with status 0, message starting `ilmek:`)
- WebSocket, SSE, progress and sent events, `withCredentials`
- Cookies, `Authorization` and Spring Security sessions (these headers are dropped)
- External URLs (they pass through to the normal network stack, outside İlmek)

If `window.ilmek` is missing, or `call` rejects, the request fails with an `HttpErrorResponse` of status 0.

## Wire contract

| Item                     | Value                                                                                                                 |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| GET, HEAD, OPTIONS       | `ilmek.call("http.read", envelope)`, no write option                                                                  |
| POST, PUT, PATCH, DELETE | `ilmek.call("http.write", envelope, { write: true })`                                                                 |
| envelope                 | `{ method, path, query?, headers?, body? }`                                                                           |
| `method`                 | uppercase                                                                                                             |
| `path`                   | URL path without the query string, e.g. `/api/tasks/1`                                                                |
| `query`                  | `{ name: [values...] }`, omitted when empty                                                                           |
| `headers`                | lowercase names; `cookie`, `authorization`, `host`, `content-length`, `origin`, `referer` dropped; omitted when empty |
| `body`                   | any JSON value, only for `http.write` with a body                                                                     |
| response                 | `{ status, headers?, body?, text? }`                                                                                  |
| status 200-399           | `HttpResponse`; body is `body`, else `text`, else `null`                                                              |
| status >= 400            | `HttpErrorResponse` with `error` = body or text                                                                       |
| `call` rejects           | `HttpErrorResponse`, status 0, `error` = original message                                                             |

## Scripts

`bun run build` (tsc to `dist/`), `bun run test`, `bun run typecheck`, `bun run lint`.

## Install gotcha: one Angular copy only

With a `file:` dependency, npm symlinks this package by default. Its own `@angular/common` and
`@angular/core` dev dependencies then resolve next to the package, so the app gets two Angular copies and
`HttpClient` silently ignores the interceptor (the request falls through with
`Failed to parse URL from /api/...`). Put `install-links=true` in the app's `.npmrc` (as
`examples/rest-notes/frontend-angular` does), or install a packed tarball.
