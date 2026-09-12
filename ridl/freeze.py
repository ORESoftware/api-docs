"""Write generated artifacts as read-only files (chmod a-w / 0444).

Git does not store the Unix write bit. After clone, re-run `ridl generate`
or `scripts/freeze-generated.sh`.
"""

from __future__ import annotations

import os
import stat
from pathlib import Path

README_NAME = "README.md"


def write_frozen(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_file():
        try:
            existing = path.read_text(encoding="utf-8")
        except OSError:
            existing = None
        if existing == text:
            freeze(path)
            return
        unfreeze(path)
    path.write_text(text, encoding="utf-8")
    freeze(path)


def freeze(path: Path) -> None:
    if not path.is_file():
        return
    mode = path.stat().st_mode
    path.chmod(mode & ~stat.S_IWUSR & ~stat.S_IWGRP & ~stat.S_IWOTH)


def unfreeze(path: Path) -> None:
    if not path.is_file():
        return
    path.chmod(path.stat().st_mode | stat.S_IWUSR)


def freeze_tree(root: Path, *, skip_readme: bool = True) -> None:
    if not root.is_dir():
        return
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [name for name in dirnames if name not in {".git", "node_modules", "target"}]
        for name in filenames:
            if skip_readme and name == README_NAME:
                continue
            freeze(Path(dirpath) / name)


FROZEN_README = """# `generated/` — frozen artifacts (read-only)

This tree is **generated** by [`ridl`](https://github.com/oresoftware/api-docs)
(`ridl generate`) and/or JSON Schema projections. Do not hand-edit adapters.

## Read-only on disk

After generate, artifact files are `chmod a-w` (0444). Git does not store
the Unix write bit (only 100644 vs 100755), so clones come back writable.
Restore with `ridl generate` or `scripts/freeze-generated.sh`.

## JSON Schema (the contract)

`json-schema/` (here or in the api-docs repo) is JSON Schema 2020-12.
Compile-time types are generated from the route map; runtime `validate()` /
schema checks must pass on real payloads. Unit tests should include valid
and invalid instances (missing required fields, wrong types, extra keys).

```sh
ridl check
ridl drift
python3 scripts/check-route-sync.py
```
"""
