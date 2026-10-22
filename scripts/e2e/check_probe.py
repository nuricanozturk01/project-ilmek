#!/usr/bin/env python3
"""Checks the reports written by the hostile isolation-probe pages (see isolation-probe.sh)."""
import json
import os
import sqlite3
import sys
from pathlib import Path

BLOCKED = lambda value: isinstance(value, str) and value.startswith("blocked")

EXPECT = {
    "fetch_external": BLOCKED,
    "fetch_no_cors": BLOCKED,
    "xhr": BLOCKED,
    "websocket": BLOCKED,
    "file_url": BLOCKED,
    "other_app_origin": BLOCKED,
    "eval": BLOCKED,
    "function_constructor": BLOCKED,
    "inline_script": lambda v: v == "blocked:inert",
    "form_post": lambda v: v == "stayed",
    "popup": BLOCKED,
    "navigation": lambda v: v == "stayed",
    "shell_state": BLOCKED,
    "shell_open": BLOCKED,
    "shell_join": BLOCKED,
    "event_listen": BLOCKED,
    "window_create": BLOCKED,
    "bridge_foreign_field": lambda v: v == "rejected:INVALID_MESSAGE",
    "bridge_shell_op": lambda v: v == "rejected:INVALID_MESSAGE",
    "sdk_replace": lambda v: v in ("immutable",) or BLOCKED(v),
    "sdk_redefine": BLOCKED,
    "bootstrap_hidden": lambda v: v == "hidden",
    "clipboard": BLOCKED,
    "geolocation": BLOCKED,
    "camera": BLOCKED,
    "beacon": lambda v: v in ("QUEUED", "blocked:refused"),
    "external_script": lambda v: v == "issued",
    "iframe": lambda v: v == "issued",
    "image": lambda v: v == "issued",
}


def report_of(database: Path) -> dict:
    with sqlite3.connect(database) as db:
        row = db.execute("SELECT value FROM kv WHERE key = 'report'").fetchone()
    return json.loads(row[0]) if row else {}


def find_databases(root: Path) -> list[Path]:
    return sorted((root / "storage").rglob("*.db"))


def main() -> int:
    client_root, canary_log = Path(sys.argv[1]), Path(sys.argv[2])
    databases = find_databases(client_root)
    failures: list[str] = []
    reports = {}
    for database in databases:
        report = report_of(database)
        if report:
            reports[database.stem] = report["results"]
    if len(reports) != 2:
        print(f"expected two probe reports, found {len(reports)}: {list(reports)}")
        return 1

    if os.environ.get("PROBE_VERBOSE"):
        print(json.dumps(reports, indent=2, sort_keys=True))
    for app_id, results in reports.items():
        if "fatal" in results:
            failures.append(f"{app_id}: probe crashed: {results['fatal']}")
            continue
        for name, ok in EXPECT.items():
            if name not in results:
                failures.append(f"{app_id}: no result for {name}")
            elif not ok(results[name]):
                failures.append(f"{app_id}: {name} = {results[name]!r}")
        if results.get("own_secret") != f"secret-of-{app_id}":
            failures.append(f"{app_id}: sees another application's storage: {results.get('own_secret')!r}")
        if not str(results.get("origin", "")).startswith("ilmekapp://app-"):
            failures.append(f"{app_id}: unexpected origin {results.get('origin')!r}")
        if results.get("local_storage_marker") != "mine":
            failures.append(f"{app_id}: localStorage did not work")

    # Web storage must start empty every time: pages run in ephemeral WebViews, so nothing
    # survives a window, a restart or a change of identity.
    for app_id, results in reports.items():
        if results.get("local_storage_before") != "none":
            failures.append(f"{app_id}: web storage persisted between runs: {results.get('local_storage_before')!r}")

    hits = canary_log.read_text().splitlines() if canary_log.exists() else []
    if hits:
        failures.append(f"the canary listener received {len(hits)} request(s): {hits[:5]}")

    for line in failures:
        print("FAIL", line)
    print(f"{len(reports)} probe(s), {len(EXPECT)} checks each, canary hits: {len(hits)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
