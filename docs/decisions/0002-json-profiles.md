# 0002 — Two strict-JSON profiles: metadata and body

## Context

PROTOCOL.md §4 lists JSON restrictions "JSON metadata'da": no duplicate keys, BOM, invalid UTF-8,
NaN/Infinity, parser-dependent numbers, nesting deeper than 16, and at most 256 object keys and
256 array items. The same section says the integer type `U53` forbids floats and exponents.

Taken literally for every JSON document, this contradicts other normative text:

- §15 allows up to 1024 files in `manifest.json`, which is a JSON array.
- §9 and §12 make the RPC body "UTF-8 JSON object" whose business schema belongs to the
  backend ("Uygulama RPC JSON gövdesinin iş şeması ilgili backend'e aittir"), so applications
  must be able to send ordinary JSON such as arrays above 256 items or decimal numbers.

## Decision

`strict_json` has two profiles:

| Rule | Metadata (frame metadata) | Body (RPC/event/list bodies, `manifest.json`) |
| --- | --- | --- |
| BOM, invalid UTF-8, duplicate keys, NaN/Infinity | rejected | rejected |
| Nesting > 16 | rejected | rejected |
| Floats, negative numbers, integers > 2^53-1 | rejected | accepted (backend-defined) |
| > 256 keys / array items | rejected | accepted (bounded by the 256 KiB body limit) |

The protocol's own typed fields keep their exact grammar in both profiles because the typed
structs validate them (`U53` fields are `u64`, hashes, UUIDs, names, ...). Hash and signature
inputs are always the exact bytes, never a re-serialization.

## Consequences

- Behaviour on the wire for protocol-defined fields is unchanged; only application payload
  bodies and the manifest are less restricted than a literal reading would make them.
- The spec owner should confirm this reading and narrow the wording of §4 to "metadata".
