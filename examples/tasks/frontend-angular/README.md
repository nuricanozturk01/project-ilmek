# Tasks frontend (Angular)

Angular frontend of the `examples/tasks` todo example: a single `App` component with a remaining
count, add (trimmed, at most 200 characters), toggle, inline edit (Enter saves, Escape cancels),
delete, clear completed and an All / Active / Completed filter. Every write reloads the list and
`tasks.events` also triggers a reload. It uses only `window.ilmek.call` (writes pass `{ write: true }`)
and `window.ilmek.subscribe`. Overview and publishing steps: [JAVA_ANGULAR](../../../docs/JAVA_ANGULAR.md).

## Constraints

- **No service worker** and no PWA schematics.
- **No external URLs:** no CDN fonts, remote scripts or stylesheets.
- **No inline CSS or scripts:** the content policy blocks them, so the production build sets
  `optimization.styles.inlineCritical` and `fonts` to `false`.
- **Unverified:** the component uses the default `ViewEncapsulation.Emulated`; Angular may inject
  component styles as `<style>` elements at runtime, which the content policy could block. If that
  happens, switch to `ViewEncapsulation.None` with the global `src/styles.css`.

## `window.ilmek`

The app uses only the contract in `src/types/ilmek-sdk.d.ts`, a type-only copy of `IlmekAppSDK`
from `packages/sdk-js/src/types.ts`. The runtime object is injected by the client; the app never
imports or bundles the SDK. It does not use `connection`, `storage` or `app`.

## Build and test

```sh
npm ci
npm run build     # dist/frontend-angular/browser/, consumed by scripts/packaging/pack-angular-app.sh
npm test          # 23 tests (Vitest with jsdom, no browser); window.ilmek is mocked
```

Every `@angular/*` package is pinned to an exact version in `package.json`; `package-lock.json` is
committed.

## Verification

Linux x86_64, Angular 22.2.4.

| Check | Status |
| --- | --- |
| Unit tests (list, add, complete, event refresh, empty state, error line) | PASS (23) |
| Production build: `index.html` has 0 `<style>` tags and 0 inline scripts | PASS |
| Desktop client window opens `ilmek://nuri/tasks` | PASS (window present) |
| Styled rendering (list, add, complete, live update) in the client | NOT OBSERVED: the screenshot did not show the window |
