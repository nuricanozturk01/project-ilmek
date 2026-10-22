#!/usr/bin/env python3
"""Waits until at least N isolation-probe reports exist under a client data root."""
import sqlite3
import sys
import time
from pathlib import Path


def reports(root: Path) -> int:
    found = 0
    for database in (root / "storage").rglob("*.db"):
        try:
            with sqlite3.connect(database) as db:
                found += 1 if db.execute("SELECT 1 FROM kv WHERE key = 'report'").fetchone() else 0
        except sqlite3.Error:
            continue
    return found


root, wanted, seconds = Path(sys.argv[1]), int(sys.argv[2]), float(sys.argv[3])
deadline = time.monotonic() + seconds
while time.monotonic() < deadline:
    if reports(root) >= wanted:
        sys.exit(0)
    time.sleep(0.3)
sys.exit(1)
