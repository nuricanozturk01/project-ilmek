# Security

**This branch (`feat/simple-no-security`) has no security.** That was a deliberate decision to get
the whole system working first ([decision 0005](decisions/0005-simple-no-security.md)). Use it only
on a network where you trust everyone who can reach the machines. The secure design (TLS 1.3,
community CA and certificates, enrollment tokens, signed packages, revocation) is on `main`.

## What is not there

| Missing | Consequence |
| --- | --- |
| Encryption | Anyone on the network path can read and change all traffic, including application data |
| Authentication | A device says who it is in HELLO (`client_id`); nothing proves it. Anyone can claim any id |
| Authorization | Any device that can reach the node can join, call every application, register hosts and publish |
| Package signatures | A host or an attacker in the path can serve a package and a registry record that agree with each other; clients cannot tell it is not the publisher's |
| Revocation | There is no way to remove a device |
| Discovery authentication | Whoever answers the UDP probe first is taken as the node. A rogue machine on the network can impersonate it |
| Rate limiting of joins | A device can register unlimited hosts and apps |

Treat the node's machine like any machine on an open network: install nothing there that you would
not run for a stranger, because anybody can publish an application whose backend you then run
(`app set-backend` is an *administrator* action on the host, which limits this, see below).

## What does still protect you

- **Integrity against corruption, not malice.** The registry record holds the manifest's SHA-256;
  the client checks the manifest bytes and then every file's size and SHA-256 before it activates
  a package. Truncated, damaged or altered-by-accident downloads are rejected, interrupted ones
  resume only from verified bytes, and a failed or tampered update leaves the active version
  running (tests `a_tampered_package_is_rejected_and_leaves_nothing_behind`,
  `a_package_altered_on_the_host_is_rejected_by_the_client_and_never_cached`,
  `a_failed_update_never_disturbs_the_active_version`). It does **not** stop a host that changes
  the manifest *and* the record.
- **Path safety.** Package paths reject `..`, absolute paths, separators, control characters,
  Windows device names and case collisions; symlinks are refused.
- **Backends need an administrator.** A package never carries an executable. A backend is chosen
  on the host with `ilmek app set-backend`, so publishing alone does not make the host run new
  code. A backend is started with its configuration on **stdin** and connects to loopback only. It
  is a *trusted* local process running with the host's operating-system privileges: a separate
  process is not a sandbox.
- **Namespace ownership.** The first `client_id` to register a namespace owns it; other ids are
  refused (`FORBIDDEN`). Because `client_id` is unauthenticated this prevents mix-ups, not attacks.
- **Bounded resources.** Frame limits, bounded queues, per-session in-flight and event windows,
  connection limits and timeouts remain (PROTOCOL.md §19).
- **Application pages stay isolated** (below).

## Application pages (untrusted HTML/JS)

Removing network security does not make application pages trusted. The page can only reach the
network through the SDK bridge. In the desktop client it runs in a sandboxed `<iframe>` of the shell (opaque origin: no cookies,
no web storage, no access to the shell's DOM or commands) and is served from a custom scheme with:

```
Content-Security-Policy: default-src 'none'; script-src ilmekapp: http://ilmekapp.localhost;
  style-src …; img-src … data: blob:; font-src …; media-src …; connect-src 'none';
  frame-src 'none'; child-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none';
  frame-ancestors tauri://localhost http://tauri.localhost http://localhost:1420;
  sandbox allow-scripts allow-forms
X-Content-Type-Options: nosniff   Cache-Control: no-store   Referrer-Policy: no-referrer
Cross-Origin-Resource-Policy: cross-origin
Permissions-Policy: camera=(), microphone=(), geolocation=(), clipboard-read=()
```

Resources are allowed by scheme, not `'self'`: in a sandboxed document WebKit takes `'self'` to be
the opaque origin and blocked the page's own files (observed). CORP had to become `cross-origin`
for the same reason (an opaque origin is never "same-origin"). Consequence: a page could ask for
files of *another currently open* application's package by its host name; those are static package
files, never data. The only channel out is `postMessage` to the shell, which relays a request only
for the frame it came from. `window.confirm`/`alert` do not work in the sandbox (apps confirm in
place). The page receives only its own verified files. The bridge has **no field** for an
application ID, host, address or file path, so a page cannot name another application or an
arbitrary destination. A package only gets the capabilities its manifest declares: `app.call`,
`app.subscribe`, `storage.local`. Limits: 200 000 bytes per call input, 1–60 s call timeout,
storage values up to 256 KiB and 5 MiB per application, scoped by community, device and application.

Verified in earlier work on `main` with `scripts/e2e/isolation-probe.sh` (Linux / WebKitGTK only,
29 checks per hostile probe) for the **old one-window-per-app model**. **That script was not re-run
and no longer matches the iframe model**; hostile-page behaviour in the iframe is covered only by
SDK unit tests (forged replies from other sources are ignored) and a UI test (only the frame's own
messages are relayed). The CSP/SDK route are unit-tested (`crates/ilmek-runtime/tests`, 15 tests); the unit tests of the
bridge and content policy (`crates/ilmek-runtime/tests`, 14 tests) pass. Not verified at all:
Windows (WebView2), macOS (WKWebView), Android WebView.

## Secrets

There are no keys, certificates or tokens in this branch. Device ids and the community profile are
plain files. Logs carry request IDs, error classes, durations and sizes, not bodies.

## How security would be added

The protocol is strict about unknown fields, so adding identity is a new protocol version, not a
patch. The intended order, from `main`'s design: TLS 1.3 between all endpoints; a community CA and
certificates in place of `client_id`; an enrollment step replacing "anyone can join"; signed
manifests; revocation. Each step needs its own decision record.
