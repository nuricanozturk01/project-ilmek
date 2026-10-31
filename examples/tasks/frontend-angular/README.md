# Tasks frontend (Angular)

Angular frontend for the `examples/tasks` backend, a single `App` component. It shows a "Todo" list with a remaining count, adds tasks (trimmed, max 200 characters, empty input ignored), toggles completion, edits titles inline (Enter saves, Escape cancels), deletes tasks, clears completed tasks, and filters All / Active / Completed on the client. Every write reloads the list, and `tasks.events` also triggers a reload. It uses only two methods of `window.ilmek`: `call` (for `tasks.list`, `tasks.create`, `tasks.complete`, `tasks.update`, `tasks.delete`, `tasks.clear_completed`; writes pass `{ write: true }`) and `subscribe` (for `tasks.events`).

## Constraints

- **No service worker.** `@angular/service-worker` and PWA schematics are not used. The app runs only inside the İlmek native runtime.
- **No external URLs.** `index.html`, styles, and scripts reference only local files. No CDN fonts, remote scripts, or remote stylesheets.
- **Inline CSS and scripts are off.** The İlmek shell CSP blocks inline `<style>` and inline `<script>`. The production build therefore sets `optimization.styles.inlineCritical` to `false` (and `fonts` to `false`), so `index.html` contains only external `<link>` and `<script src>` tags.
- **Encapsulation risk (unverified).** The component uses Angular's default `ViewEncapsulation.Emulated`. Angular may still inject component styles as `<style>` elements at runtime, which the shell CSP could block. This is the one unverified risk; Task 13 tests it in the real client. If it fails, switch to `ViewEncapsulation.None` with the global `src/styles.css`.

## window.ilmek contract

The app uses only the contract declared in `src/types/ilmek-sdk.d.ts`, a type-only copy of `IlmekAppSDK` from `packages/sdk-js/src/types.ts`. The runtime object is injected by the shell. The app never imports or bundles the SDK.

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

## Verification

Environment: Linux x86_64, Angular 22.2.4 build, Tauri client built from `apps/client/src-tauri` (Linux, DISPLAY :0). The backend is the real `backend-java.jar` run by the real host (see `examples/tasks/backend-java/README.md`).

| Check | Status | Command | Observed |
| --- | --- | --- | --- |
| Angular unit tests | PASS (from Task 12) | `npm test` | `Tests 8 passed (8)` |
| Angular production build | PASS (from Task 12) | `npm run build` | `index.html` has 0 `<style>` and 0 inline scripts |
| Tauri client build | PASS | `cargo build -p ilmek-client-app --manifest-path src-tauri/Cargo.toml` (from `apps/client`) | `Finished` with exit 0 |
| Client window opens | PASS | `target/debug/ilmek-client "ilmek://nuri/tasks"` | window `İlmek` present, geometry 1100x760 |
| Todo UI renders styled (list, add, complete, live update) | BLOCKED | screenshot via `gnome-screenshot -f` after launch | captures showed only the desktop wallpaper; the İlmek window did not appear in the captures, so rendering and styling were not observed |

The ViewEncapsulation (Emulated) styling question is therefore unresolved. Do not treat the styling as verified. If the GUI check can be completed and the component styles are missing, the fix is `ViewEncapsulation.None` with the global `src/styles.css`, not a CSP change.
