# 0001 — Wire fixtures regenerated under ILMEK identifiers

## Context

`WIRE_EXAMPLES.json` (as supplied) declares `ADA/1.0` and uses magic `ADA1`, hash prefixes
`ADA-CALL-V1` / `ADA-MANIFEST-V1` and the client name `ADA test fixture`. README ("Adlandırma
sözleşmesi") and PROTOCOL.md §18 require `ILM1`, `ILMEK-CALL-V1`, `ILMEK-MANIFEST-V1`, and both
documents, plus `DOCUMENT_CHECKS.md`, state that the fixtures were already regenerated for the
rename. They were not: the file contains no `ILMEK` string at all.

The supplied file is internally consistent (hashes and signature verify under the `ADA`
prefixes), so the only defect is stale identifiers.

## Decision

- Keep the supplied file byte-for-byte as `tests/protocol/fixtures/wire_examples.legacy_ada.json`.
  It is never used as an expectation source.
- Derive `tests/protocol/fixtures/wire_examples.ilmek.json` with
  `scripts/fixtures/regen_ilmek_fixtures.py`. Only identifiers change: magic, hash prefixes,
  the `client_name` string and the `index.html` body text. UUIDs, field layout, limits and the
  public test-only Ed25519 seed are unchanged. Dependent values (asset hash, manifest bytes and
  hash, signature, payload hash, lengths) are recomputed from the documented preimages.
- The script first proves its serializer reproduces every legacy frame byte-for-byte.

## Consequences

- No normative wire behaviour changes; this is an editorial correction.
- The spec documents still claim the fixtures were regenerated. They should be corrected by the
  document owner; this repo does not edit the supplied specs.
- Hash/signature values in the ILMEK fixtures come from this script, not from the spec author.
  Independent reproduction by a second implementation is still pending.
