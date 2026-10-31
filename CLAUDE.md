# Ilmek — Claude Code Implementation Instructions

## Mission

Implement Ilmek as a real, installable, internet-independent community application network. Work in this repository, write the implementation, run the available tests, and deliver usable artifacts. Do not stop after producing a plan, architecture description, UI mockup, or empty project skeleton.

This file governs the implementation workflow. It supplements the supplied specifications; it does not replace them or introduce a different wire protocol.

## Required reading

Before implementation, inspect the repository, its existing changes, and applicable repository instructions. Read these four files in full:

1. `README.md` — product requirements, topology, platforms, hosting, delivery, and acceptance tests.
2. `PROTOCOL.md` — normative wire format, messages, identity, authorization, state machines, limits, and failure behavior.
3. `WIRE_EXAMPLES.json` — deterministic byte, hash, and signature fixtures.
4. `DOCUMENT_CHECKS.md` — checks already performed and their explicit limitations.

Place this file alongside those files at the repository root. If the specifications have already been moved into a documented requirements directory, locate and use the authoritative copies. Do not invent missing specification content.

For protocol behavior, `PROTOCOL.md` is authoritative. For product scope and physical deployment, the supplied README is authoritative. Existing fixture validation is not evidence that an implementation, a platform, or a physical network has been tested.

Preserve the original requirements when turning the README into operational documentation; for example, retain them in `docs/REQUIREMENTS.md` and update references consistently. Do not maintain conflicting copies.

## Product requirements

- Normal operation must work without the public internet, cloud services, or mobile data.
- Two old computers act as Linux routing nodes. Two Android phones and three Windows/macOS/Linux computers are the initial end devices.
- Both client access and the connection between routing nodes use Wi-Fi. Bluetooth is outside V1.
- A publisher can host applications on an independent reachable computer. Applications are not required to run on the shared routing nodes.
- A host supports multiple applications through application identity and independent lifecycles. Capacity depends on hardware and workload; do not impose a fixed product-level application count.
- Clients resolve an application name and connect to its host. The registry must not proxy all application traffic.
- Applications support HTML, CSS, and JavaScript through a native client runtime with enforced isolation.
- Deliver real socket communication, real application execution, and persistent data. Browser-only simulations and fabricated network activity do not satisfy the task.

## Fixed identifiers and architecture

| Item | Required value |
| --- | --- |
| Display name | İlmek |
| English protocol name | Ilmek Protocol |
| Protocol major version | `ILMEK/1` |
| Network stack | Wi-Fi → IP → TCP → TLS 1.3 → ILMEK/1 |
| TLS ALPN | `ilmek/1` |
| Four-byte frame magic | `ILM1` |
| Application URI scheme | `ilmek://` |
| CLI command | `ilmek` |
| JavaScript/TypeScript SDK | `@ilmek/sdk` |
| SDK interface name | `IlmekAppSDK` |
| RPC hash domain prefix | ASCII `ILMEK-CALL-V1`, followed by one NUL byte |
| Manifest signature domain prefix | ASCII `ILMEK-MANIFEST-V1`, followed by one NUL byte |
| UUID fields | RFC 9562 UUIDv7 |

Read the complete hash preimages, framing rules, and identity rules from `PROTOCOL.md`. Do not derive them from this summary alone. Use ASCII `ilmek` in technical identifiers and the specified `ilmek-*` component names. Do not revive previous provisional names or silently accept their protocol identifiers.

Reuse standard Wi-Fi drivers, IP routing, TCP, maintained TLS/cryptography libraries, and platform rendering engines. The custom protocol operates at the application layer. It does not replace the physical radio, IP, or TCP.

Do not substitute HTTP, HTTPS, WebSocket, gRPC tunnels, or public-internet tunnels for ILMEK/1 application traffic. Development tooling and internal WebView resource mapping must respect the exceptions and boundaries in the specifications; neither may become a production network fallback.

## Implementation approach

Use the documented Rust shared core, asynchronous networking, native client architecture, TypeScript UI/SDK, platform adapters, and persistence choices. Consult current official dependency documentation and pin compatible versions in lockfiles. Do not assume an API or platform feature exists.

Keep protocol, identity, registry, host management, networking setup, application SDKs, and UI responsibilities separate. Follow the repository layout in the requirements without creating unnecessary empty crates or speculative abstractions.

Proceed through these stages:

1. **Repository and environment:** inspect existing code and user changes; inventory accessible hardware, operating systems, toolchains, and build targets. Map requirements to implementation tasks and acceptance checks.
2. **Real protocol path:** implement bounded framing, validation, UUIDv7 handling, TLS identity checks, handshake/state handling, and an actual socket transfer between separate processes. Import the supplied fixtures into tests.
3. **Registry and identity:** implement bootstrap, enrollment, trust refresh, authorization, names, application records, leases, and persistence according to the specification.
4. **Multi-application host:** implement package handling, verification, application lifecycle management, backend IPC, isolated data directories, resource bounds, and durable application operations.
5. **Native clients and SDKs:** implement resolution, downloads, verification, controlled updates, isolated HTML execution, runtime bridge permissions, and the documented desktop/Android clients.
6. **Example applications:** run the required applications with actual backends and persistent data where applicable. Demonstrate multiple applications on one host and an independent application on another host.
7. **Network and delivery:** implement configuration, diagnostics, deployment/rollback tooling, packaging, backup/restore, operational documentation, and the physical-device test procedure.

Develop and verify vertical slices throughout these stages. Do not finish a polished interface while leaving its core functionality simulated or unimplemented.

## Decision making and continuity

Make routine, reversible implementation decisions independently and record important reasoning. Maintain concise progress notes with completed work, evidence, blockers, and the next executable steps so work can resume after a context reset.

If a specification contains an ambiguity or contradiction, identify the exact affected requirements. Resolve editorial inconsistencies without changing behavior. For a change to product scope, normative wire behavior, or security guarantees, document the proposed resolution and obtain approval before applying that change. Continue independent, unblocked implementation work.

Do not weaken specifications or rewrite fixture expectations merely to make failing tests pass. Investigate whether the implementation, fixture, or specification is wrong and preserve an explicit record of the resolution.

Respect actual tool permissions and approval boundaries. Do not claim to continue running after the execution environment has stopped. Preserve completed work and leave a concrete continuation record when blocked.

## Security and correctness

- Never disable certificate verification or introduce an insecure production fallback.
- Keep secrets out of source control, example configuration, command-line arguments, and logs. Deterministic fixture keys are test data only.
- Derive authenticated identity from the verified connection. Application-supplied identity fields must not override it.
- Enforce community, principal, publisher, application, method, and subscription boundaries as specified.
- Validate lengths before allocation. Bound queues, concurrent work, retries, downloads, and process restart behavior.
- Preserve operation IDs and original request body bytes across retries. Do not infer rollback from a timeout or a disconnected socket.
- Commit business changes and their durable deduplication results atomically where required.
- Verify manifests, signatures, paths, sizes, and hashes before activating packages. Interrupted or invalid updates must not corrupt the active version.
- Treat downloaded HTML/JavaScript as untrusted. Do not expose general native commands, shell access, arbitrary files, or arbitrary network destinations through the bridge.
- Distinguish trusted local backend processes from sandboxed application content. A separate process is not, by itself, a security sandbox.

## Testing requirements

Test real behavior rather than only mirroring implementation details. Include:

- Fixture compatibility, partial and combined TCP reads, invalid metadata, oversized frames, EOF, and timeout handling.
- Invalid ALPN, CA, certificate validity, IP SAN, peer identity, and authorization cases.
- Registry operations, name ownership, trust snapshots, and cache expiry behavior.
- Concurrent duplicate writes, conflicting retries, and connection loss around commits.
- Backend crashes, independent application lifecycles, and persistent data recovery.
- Download interruption, cancellation races, resume validation, tampering, and atomic activation.
- SDK permissions, application isolation, and platform-specific WebView boundaries.
- Database migrations and a real backup/restore exercise.

Maintain a traceable status for every A01–A24 product acceptance item and P01–P24 protocol conformance item. Use explicit statuses such as PASS, FAIL, BLOCKED, and NOT RUN. Each executed check must identify the environment, revision, command or procedure, and observed result.

A loopback test is not a Wi-Fi test. A successful build is not a runtime test. One platform's successful build does not validate another platform. Fixture checks do not validate the complete protocol state machine.

## Hardware and privileged operations

Do not assume the available computers have the required Wi-Fi adapters, concurrent interface modes, AP support, drivers, or radio coverage. Report observed capabilities and missing information.

Before changing network interfaces, routes, DHCP, firewall rules, services, or operating-system configuration, prepare an exact plan, preserve the existing configuration, and provide a rollback path. Obtain required authorization and privileges. Do not disable the entire firewall, reinstall an OS, erase data, or risk losing the only management connection as a default solution.

If physical devices or target platforms are unavailable, continue all independent software work. Produce the available builds, automated tests, setup tools, and reproducible device-test instructions. Mark unavailable physical/platform checks explicitly. Do not replace the two-node requirement with a single simulated network.

## Deliverables and completion

Deliver source code, lockfiles, configuration schemas/examples, SDKs, example applications, database migrations, packaging tools, available installable artifacts, and the documentation required by the specifications.

Replace speculative usage instructions with commands verified against the implemented tools. Clearly label instructions that have not been executed in the available environment. Report which operating systems and architectures were actually built and tested.

The final handoff must contain:

1. Implemented functionality and important design decisions.
2. Verified build, start, publish, connect, and test instructions.
3. Executed tests and their results.
4. Produced artifacts and their target platforms.
5. The complete acceptance/conformance status matrix.
6. Remaining defects, blocked checks, and the exact hardware, access, or decisions needed to finish them.

Do not describe stubs, mocked flows, unexecuted tests, or inaccessible hardware as complete. Do not claim production readiness, perfect security, or guaranteed performance without supporting evidence. Completion is determined by the specification's acceptance criteria.

When asked to begin implementation, read the required files, establish the current repository state, present a short execution plan, and immediately start the first implementation step. Continue through available work rather than returning only recommendations.

## Explanation series (`açıklama/`)

When the user says "açıklamaya devam et", "devam et", "başla" about the explanation documents, continue the series in `açıklama/`. Do not start new implementation work.

1. Read `açıklama/açıklama_plan.md` first. The `next:` line in its code block names the document to write. Never guess the next document from memory.
2. Follow that plan file for the document's topic, source files, structure and the accuracy rules (read every assigned source file fully before writing; cite `file:line`; label claims `[KOD]`, `[ÇALIŞTIRILDI]`, `[BELGE]`, `[TEST EDİLMEDİ]`, `[YORUM]`; no invented output; audit pass after writing).
3. Write the document in Turkish under `açıklama/` with the exact file name from the plan.
4. After each document, update `açıklama_plan.md`: `prev:` = the document written before this one, `now:` = the document just written, `next:` = the following document in the plan table; also update the progress table.
5. Change the plan itself (document list, coverage table) only by editing the plan file first.
6. Report briefly what was written, any code/doc inconsistencies found, and what `next:` is now. Write one document per request unless told otherwise.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- A committed graphify-out/graph.json is rewritten whole on every build, so a large diff to it in a pull request is expected. Do not flag it as churn or suggest skipping the graph rebuild or its hooks.
