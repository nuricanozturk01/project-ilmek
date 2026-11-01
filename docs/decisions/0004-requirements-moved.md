# 0004 — Product specification moved to docs/REQUIREMENTS.md

## Context

The implementation instructions (CLAUDE.md) require the original product specification to be
preserved as `docs/REQUIREMENTS.md` and `README.md` to become operational documentation, with
references updated and no conflicting copies.

## Decision

- `README.md` was moved with `git mv` to `docs/REQUIREMENTS.md`. Its text and section numbers are
  unchanged, so every `REQUIREMENTS §N` reference in code and docs points at the same section the
  old `README §N` did.
- `PROTOCOL.md` has two editorial reference changes: the header link and the sentence naming the
  normative product document now say `docs/REQUIREMENTS.md`. No normative behavior changed.
- `README.md` is a new, short, operational document. Where it and REQUIREMENTS differ about
  scope, REQUIREMENTS is authoritative.
- ADR 0001 describes a historical state ("README" at the time) and is left as written. The
  document-check log `DOCUMENT_CHECKS.md` it mentions was later removed from the repository.

## Consequences

Readers of the old README text find it at the new path. No second copy exists.
