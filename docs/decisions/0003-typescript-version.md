# 0003 — TypeScript 6.0 for the frontend toolchain

## Context

CLAUDE.md asks for the current stable release of every tool. The newest `typescript` on npm is
7.0.x, but `typescript-eslint` (needed for the mandated `strict-type-checked` ESLint profile)
refuses to start under it: "typescript-eslint does not support TS 7.0" (its tracking issue
targets TS >= 7.1).

## Decision

Pin `typescript` to the newest 6.x (6.0.3) in all packages until typescript-eslint supports 7.x.
Lint rules are not weakened to make the newer compiler usable.

## Consequences

Revisit when typescript-eslint supports TypeScript 7; the change is a one-line version bump per
package plus a lockfile update.
