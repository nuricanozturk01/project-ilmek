#!/usr/bin/env python3
"""Regenerate WIRE_EXAMPLES.json under the ILMEK/1 identifiers.

The supplied WIRE_EXAMPLES.json is internally consistent but still carries the
provisional `ADA` identifiers (magic `ADA1`, hash prefixes `ADA-CALL-V1` /
`ADA-MANIFEST-V1`). PROTOCOL.md requires `ILM1`, `ILMEK-CALL-V1` and
`ILMEK-MANIFEST-V1`. See docs/decisions/0001-fixture-rename.md.

Only identifiers change; field layout, limits and the (public, test-only) Ed25519
seed are kept. Output is deterministic (Ed25519 signatures are deterministic).

Usage: python3 -I scripts/fixtures/regen_ilmek_fixtures.py LEGACY.json OUT.json

NOT USABLE ON the simple edition (the `main` branch): it changed the message schema (HELLO client_id,
no signature or publisher_key_id) and edited tests/protocol/fixtures/wire_examples.ilmek.json by
hand-scripted patch (docs/decisions/0005-simple-no-security.md). Running this script would
produce the `security` branch schema and break the protocol tests, so it refuses to run on this branch.
"""
import hashlib
import json
import struct
import sys

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization
import base64

sys.exit("refusing to run: the fixtures of this branch use a different schema (see the docstring)")

MAGIC = b"ILM1"
CALL_PREFIX = b"ILMEK-CALL-V1\x00"
MANIFEST_PREFIX = b"ILMEK-MANIFEST-V1\x00"


def compact(obj) -> bytes:
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def frame(meta_bytes: bytes, body: bytes, magic: bytes = MAGIC) -> bytes:
    return magic + struct.pack(">II", len(meta_bytes), len(body)) + meta_bytes + body


def b64u(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def main(legacy_path: str, out_path: str) -> None:
    legacy = json.load(open(legacy_path, encoding="utf-8"))

    # Serializer self-check: re-serializing legacy metadata must reproduce legacy bytes.
    for fr in legacy["valid_frames"]:
        assert compact(fr["metadata"]) == fr["metadata_utf8"].encode("utf-8"), fr["name"]
        raw = bytes.fromhex(fr["frame_hex"])
        assert raw == frame(fr["metadata_utf8"].encode(), bytes.fromhex(fr["body_hex"]), b"ADA1")

    seed = bytes.fromhex(legacy["signature_example"]["test_only_seed_hex"])
    key = Ed25519PrivateKey.from_private_bytes(seed)
    pub = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    key_id = sha(pub)
    assert key_id == legacy["signature_example"]["publisher_key_id"]

    by_name = {f["name"]: f for f in legacy["valid_frames"]}

    # --- asset + manifest -------------------------------------------------
    index_html = '<!doctype html><html lang="tr"><body>Merhaba ILMEK</body></html>\n'.encode()
    asset_hash = sha(index_html)
    manifest = json.loads(legacy["signature_example"]["manifest_utf8"])
    manifest["files"] = [
        {"path": "index.html", "size_bytes": len(index_html), "sha256": asset_hash}
    ]
    manifest["total_size_bytes"] = len(index_html)
    manifest_bytes = compact(manifest)
    manifest_hash = sha(manifest_bytes)
    sig_pre = MANIFEST_PREFIX + bytes.fromhex(manifest_hash)
    signature = key.sign(sig_pre)
    assert len(signature) == 64

    # --- write call hash --------------------------------------------------
    wc = by_name["write_call"]
    call_body = bytes.fromhex(wc["body_hex"])
    call_pre = (
        CALL_PREFIX
        + wc["metadata"]["data"]["app_id"].encode()
        + b"\x00"
        + wc["metadata"]["data"]["method"].encode()
        + b"\x00"
        + call_body
    )
    payload_hash = sha(call_pre)

    # --- rebuild frames ---------------------------------------------------
    patches = {
        "hello_control": lambda m, b: (
            m["data"].update(client_name="ILMEK test fixture"), b)[1],
        "write_call": lambda m, b: (
            m["data"].update(payload_hash=payload_hash), b)[1],
        "get_manifest": lambda m, b: (
            m["data"].update(manifest_hash=manifest_hash), b)[1],
        "manifest": lambda m, b: (
            m["data"].update(manifest_hash=manifest_hash,
                             publisher_key_id=key_id,
                             signature=b64u(signature)), manifest_bytes)[1],
        "get_asset": lambda m, b: (
            m["data"].update(manifest_hash=manifest_hash, asset_hash=asset_hash), b)[1],
        "asset_info": lambda m, b: (
            m["data"].update(asset_hash=asset_hash, total_size=len(index_html)), b)[1],
        "asset_chunk": lambda m, b: index_html,
        "asset_end": lambda m, b: (
            m["data"].update(asset_hash=asset_hash, total_sent=len(index_html)), b)[1],
    }

    out_frames = []
    for fr in legacy["valid_frames"]:
        meta = json.loads(json.dumps(fr["metadata"]))
        body = bytes.fromhex(fr["body_hex"])
        if fr["name"] in patches:
            body = patches[fr["name"]](meta, body)
        meta_bytes = compact(meta)
        out_frames.append({
            "name": fr["name"],
            "metadata": meta,
            "metadata_utf8": meta_bytes.decode("utf-8"),
            "body_hex": body.hex(),
            "metadata_length": len(meta_bytes),
            "body_length": len(body),
            "frame_hex": frame(meta_bytes, body).hex(),
        })

    hello = next(f for f in out_frames if f["name"] == "hello_control")
    hello_raw = bytes.fromhex(hello["frame_hex"])
    dup = b'{"type":"PING","type":"PONG"}'
    invalid = [
        {"name": "bad_magic", "frame_hex": (b"BAD1" + hello_raw[4:]).hex(),
         "expected_local_decoder_error": "bad_magic"},
        {"name": "metadata_too_large",
         "frame_hex": (MAGIC + struct.pack(">II", 65537, 0) ).hex() + "00",
         "expected_local_decoder_error": "length_limit"},
        {"name": "body_too_large",
         "frame_hex": (MAGIC + struct.pack(">II", 1, 262145)).hex() + "01",
         "expected_local_decoder_error": "length_limit"},
        {"name": "truncated_frame", "frame_hex": hello_raw[:-1].hex(),
         "expected_local_decoder_error": "length_mismatch"},
        {"name": "duplicate_json_key", "frame_hex": frame(dup, b"").hex(),
         "expected_local_decoder_error": "duplicate_key"},
    ]

    out = {
        "specification": "ILMEK/1",
        "notice": "Deterministik doküman fixture verisi; üretim anahtarı veya çalışan ağ testi "
                  "değildir. Örnekler ayrı akış parçalarıdır; tek oturum transcripti değildir. "
                  "scripts/fixtures/regen_ilmek_fixtures.py ile WIRE_EXAMPLES.json'dan "
                  "(ADA/1.0) türetilmiştir; bkz. docs/decisions/0001-fixture-rename.md.",
        "valid_frames": out_frames,
        "invalid_frames": invalid,
        "hash_example": {"preimage_hex": call_pre.hex(), "payload_hash": payload_hash},
        "signature_example": {
            "test_only_seed_hex": seed.hex(),
            "public_key_base64url": b64u(pub),
            "publisher_key_id": key_id,
            "manifest_utf8": manifest_bytes.decode("utf-8"),
            "manifest_hash": manifest_hash,
            "preimage_hex": sig_pre.hex(),
            "signature_base64url": b64u(signature),
        },
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(out, fh, ensure_ascii=False, indent=2)
        fh.write("\n")
    print(f"wrote {out_path}: {len(out_frames)} valid, {len(invalid)} invalid frames")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
