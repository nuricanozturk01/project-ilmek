# Security

Scope: what the software does today, what it relies on you to do, and the limits that are real.
The wire-level rules are in [PROTOCOL.md](../PROTOCOL.md); the product requirements are in
[REQUIREMENTS.md](REQUIREMENTS.md). Nothing here has been audited, and no claim of anonymity,
perfect security or protection against someone with physical or root access to a host is made.

## Who is trusted with what

| Party | Trusted to | Not trusted to |
| --- | --- | --- |
| Community CA (the registry machine) | decide which devices exist and which names they own | — (compromise of its key is total; see "Keys") |
| Enrolled member | use applications it may reach | choose its own identity, reach other applications' data, publish |
| Publisher | sign packages for the namespaces it was granted | install on a host that was not granted to it, or on a host that does not trust its key |
| Host operator | run backends they installed with `app set-backend` | — (a backend is a trusted local process) |
| Application page (HTML/JS) | nothing | everything: see "Application pages" |
| Network between devices | nothing (TLS 1.3 end to end; routers see IPs, sizes and timing only) | confidentiality of traffic metadata |

## Enrollment and identity

- A device joins with `community.json` **plus the CA's SHA-256 fingerprint** (mandatory on
  `ilmek identity enroll`) **plus a single-use token**, all obtained out of band. A wrong
  fingerprint is refused (tested in `scripts/smoke/local-processes.sh`).
- Tokens: shown once when created (`community invite`), stored only as a hash, valid 15 minutes
  by default (at most 7 days), consumed on first use. A lost enrollment reply can be replayed with
  the same token and key and yields the same certificate; a different request with that token is
  refused. Five failed attempts from one source within a minute are rate-limited.
  Tokens are read from a file or stdin, never from the command line.
- The CA, not the device, decides the identity in the certificate (a peer UUIDv7 as a URI SAN);
  names asked for in the CSR are ignored. Hosts additionally get an IP SAN, which clients check.
- Certificates last **30 days** (registry server certificate: 30 days by default, `--identity-days`
  to change). Renew devices with `community invite --bound-peer <peer>` followed by a new
  `identity enroll`; renew the registry certificate with `community reissue-identity` and a
  restart. See [OPERATIONS](OPERATIONS.md). Renewal of a revoked or unknown peer is refused.

## Transport

TLS 1.3 only, ALPN `ilmek/1`, certificate chain to the community CA (a certificate from another CA
is refused), the server's IP SAN and expected peer ID are checked by the client, sessions are never
resumed. Verification is never disabled and there is no insecure fallback. Normal endpoints use
mutual TLS; only the enrollment endpoint accepts a client without a certificate. Tests:
`crates/ilmek-core/tests/tls.rs` (wrong ALPN, wrong CA, wrong IP, wrong peer, expired, non-TLS
bytes, no client certificate).

## Authorization

The caller's identity comes from the verified certificate and nothing else; an identity field in a
request body is never used. Every application call is checked on the host for the application ID,
the method table and (for backends) the principal the host attaches. Cursors and subscriptions are
bound to principal and topic. Names are owned by a publisher namespace; only the owner can
register, update or unregister, and a removed name stays reserved for its owner. The example notes
application returns the same error whether a note exists or belongs to someone else, so other
members cannot even detect it.

## Revocation and the offline window

Revoking a peer (`community revoke-peer`) or a publisher key (`revoke-publisher-key`) bumps a
trust snapshot. Hosts and clients fetch it every 60 s by default. A snapshot becomes effective only
after its last page and may not regress. An active session is closed at the next gate check and
the next request is refused. **Offline window:** if a device or host cannot reach the registry, it
keeps using its last snapshot for at most **24 hours**; after that remote work is refused
(`TRUST_STALE`). During those hours a revoked identity may still be accepted by a host that has not
refreshed. A clock that moves backwards makes the snapshot stale rather than fresh.

## Packages

Manifests are canonical (sorted, unique, bounded), signed with Ed25519 over a domain-separated
preimage (`ILMEK-MANIFEST-V1\0…`). The host and the client verify signature, per-file size and
SHA-256, path safety (no `..`, absolute paths, separators or control characters), no extra or
missing files, no symlinks, and size limits **before** activation. A package is staged, verified
and switched in atomically; a failed or tampered update leaves the active version in place; updates
must move forward and keep the application identity; rollback is an explicit command. A host
installs only packages signed by a publisher key the administrator trusts
(`ilmek host trust-publisher`).

## Application pages (untrusted HTML/JS)

The page can only reach the network through the SDK bridge. In the desktop client it is served
from a custom protocol with:

```
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self';
  img-src 'self' data: blob:; font-src 'self'; media-src 'self'; connect-src ipc: http://ipc.localhost;
  frame-src 'none'; child-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none';
  frame-ancestors 'none'
X-Content-Type-Options: nosniff   Cache-Control: no-store   Referrer-Policy: no-referrer
Cross-Origin-Resource-Policy: same-origin
Permissions-Policy: camera=(), microphone=(), geolocation=(), clipboard-read=()
```

(`ipc:` / `ipc.localhost` is the WebView-internal channel to the native runtime, not a network
destination.) The page receives only its own verified files. The bridge has **no field** for an
application ID, host, address or file path, so a page cannot name another application or an
arbitrary destination; the runtime fills in the verified identity. A package only gets the
capabilities its manifest declares: `app.call`, `app.subscribe`, `storage.local`. Limits: 200 000
bytes per call input, 1–60 s call timeout, storage values up to 256 KiB and 5 MiB per application,
scoped by community, device and application. `v-html`-style injection is not used in the client
UI. The WebView's own storage (cookies, localStorage, IndexedDB) is keyed by origin only, so it is
wiped when the last application window closes and again at client start-up; applications are told
to use the SDK's scoped storage instead.

**Verified:** Linux / WebKitGTK only, with `scripts/e2e/isolation-probe.sh`: two hostile probe
applications run at once; 29 checks each — `fetch` and beacons to a canary listener, `file://`
reads, another application's origin, `eval`/`Function`, inline and external scripts, iframes,
form posts, popups, navigation, the client's native shell commands and window creation, forged
bridge fields, replacing or redefining the SDK object, clipboard and camera — all failed as
intended, canary hits 0; a weakened-CSP negative control failed as expected; storage written by the first run is gone after a client
restart. **Not verified:** Windows (WebView2), macOS (WKWebView) and Android WebView. Their
isolation behavior differs, and the checks above must be re-run there.

## Backends

A backend is started by the host with its configuration (token, IPC port, application ID, data
directory) on **stdin**, never in the argument list or log. It connects to loopback only and gets
its caller's identity from the host. This is a *trusted* local process: it runs with the host's
operating-system privileges and can read anything the host user can. Run the host as an
unprivileged user (`deploy/systemd/ilmek-host.service` applies systemd hardening) and install only
backends from publishers you trust. A separate process is not a sandbox.

## Keys and secrets

- Private keys (CA, registry identity, device identity, publisher key) are written as owner-only
  files (mode 0600, directories 0700) on Unix. **On Windows no ACL hardening is implemented**:
  protect those directories with the user profile's permissions and full-disk encryption.
- The **CA key** is a plain owner-only file on the registry machine. Whoever holds it can create
  members, hosts and publishers. There is no HSM, passphrase or OS key-store integration.
- Passphrases for the Wi-Fi networks are read from root-only files at apply time; they are not in
  the plan, in `argv` or in the snapshot. They are, necessarily, in the generated `hostapd` config
  (mode 600).
- Backups: private keys are **left out** of a registry backup unless `--include-keys` is given;
  a backup with keys must be kept encrypted and apart from log archives.
- Logs carry request IDs, error classes, durations and sizes; they are written without tokens,
  keys or request bodies. Host and backend logs rotate at 1 MiB with 3 files kept.

## Network node

`ilmek-network` changes only what the plan lists, in a separate nftables table, with a snapshot and
rollback ([NETWORK_SETUP](NETWORK_SETUP.md)). It does not disable the firewall, enable NAT, add a
default route, or touch the interface carrying your default route or SSH session unless forced.
Applying it still needs root.

## Known gaps and real limits

- No physical Wi-Fi, Android, Windows or macOS test has been performed.
- Windows key-file permissions are not hardened; the CA key has no extra protection.
- A revoked device can be accepted for up to 24 hours by an isolated host; the registry is the only
  authoritative source and has no failover.
- Wi-Fi WPA2-PSK protects the radio link against outsiders; every member knows the passphrase, so
  it is not a member boundary. The member boundary is the TLS identity.
- No denial-of-service guarantees beyond bounded queues, frame limits, per-session in-flight and
  event windows, connection limits and timeouts.
- Metadata (who talks to whom, when, how much) is visible on the local network.
