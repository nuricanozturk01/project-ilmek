# Tasks frontend (Angular)

Angular frontend for the `examples/tasks` backend. It is a single `App` component that lists tasks, adds a task, completes a task, and refreshes when `tasks.events` fires.

## Constraints

- **No service worker.** `@angular/service-worker` and PWA schematics are not used. The app runs only inside the İlmek native runtime.
- **No external URLs.** `index.html`, styles, and scripts reference only local files. No CDN fonts, remote scripts, or remote stylesheets.
- **Encapsulation risk (unverified).** The component uses Angular's default `ViewEncapsulation.Emulated`. Angular injects component styles as `<style>` elements at runtime, which a strict Content-Security-Policy could block. This is the one unverified risk; Task 13 tests it in the real client. If it fails, switch to `ViewEncapsulation.None` with the global `src/styles.css`.
- **Inline critical CSS.** The production build inlines critical CSS into `index.html` as a `<style>` element. This is the same CSP consideration as above.

## window.ilmek contract

The app uses only the contract declared in `src/types/ilmek-sdk.d.ts`, a type-only copy of `IlmekAppSDK` from `packages/sdk-js/src/types.ts`. The runtime object is injected by the shell. The app never imports or bundles the SDK.

- `window.ilmek.call('tasks.list', {})`
- `window.ilmek.call('tasks.create', { title }, { write: true })`
- `window.ilmek.call('tasks.complete', { id }, { write: true })`
- `window.ilmek.subscribe('tasks.events', null, handler)`

The app does not use `connection`, `storage`, or `app`.

## Build

```sh
npm ci
npm run build
```

The build writes `dist/frontend-angular/browser/`. That directory is consumed by the packaging script.

## Test

```sh
npm test
```

Tests use Angular's default unit runner for Angular 22 (Vitest with jsdom). No browser is required. The tests mock `window.ilmek` and assert the list, add, complete, event refresh, empty state, and error line.

## Versions

Every `@angular/*` package is pinned to an exact version in `package.json`. `package-lock.json` is committed.
