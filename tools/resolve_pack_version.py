#!/usr/bin/env python3
"""Resolve a pack's release version.

Order: explicit --override, then ci/provider-matrix.json, then the pack's own
pack.yaml. Never the workspace version — see docs/release-policy.md.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def matrix_version(root: Path, pack_name: str) -> str | None:
    matrix = root / "ci" / "provider-matrix.json"
    if not matrix.exists():
        return None
    providers = json.loads(matrix.read_text()).get("providers") or {}
    for entry in providers.values():
        if isinstance(entry, dict) and entry.get("pack") == pack_name:
            version = entry.get("version")
            if version:
                return str(version)
    return None


def pack_yaml_version(pack_dir: Path) -> str | None:
    pack_yaml = pack_dir / "pack.yaml"
    if not pack_yaml.exists():
        return None
    for line in pack_yaml.read_text().splitlines():
        if line.startswith("version:"):
            return line.split(":", 1)[1].strip().strip("\"'") or None
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pack_dir", type=Path)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--override", default="")
    parser.add_argument(
        "--source",
        choices=("auto", "pack-yaml"),
        default="auto",
        help="pack-yaml reads only the pack's own declaration, ignoring the matrix.",
    )
    args = parser.parse_args()

    if args.override:
        print(args.override)
        return 0

    pack_dir = args.pack_dir.resolve()
    if args.source == "pack-yaml":
        version = pack_yaml_version(pack_dir)
    else:
        version = matrix_version(args.root.resolve(), pack_dir.name) or pack_yaml_version(pack_dir)
    if not version:
        print(f"cannot resolve a version for {pack_dir.name}", file=sys.stderr)
        return 1
    print(version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
