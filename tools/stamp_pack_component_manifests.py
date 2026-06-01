#!/usr/bin/env python3
"""Stamp bundled pack component manifests to the pack version."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import yaml


def load_component_metadata(pack_yaml: Path) -> dict[str, dict[str, Any]]:
    data = yaml.safe_load(pack_yaml.read_text()) if pack_yaml.exists() else {}
    result: dict[str, dict[str, Any]] = {}
    for component in (data or {}).get("components", []) or []:
        if isinstance(component, dict) and component.get("id"):
            result[str(component["id"])] = component
    return result


def stamp_manifest(path: Path, version: str, metadata: dict[str, dict[str, Any]]) -> bool:
    data = json.loads(path.read_text())
    changed = data.get("version") != version
    data["version"] = version

    component = metadata.get(str(data.get("id") or ""))
    if component:
        for key in ("world", "profiles"):
            if key in component and data.get(key) != component[key]:
                data[key] = component[key]
                changed = True

    if changed:
        path.write_text(json.dumps(data, indent=2) + "\n")
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pack_dir", type=Path)
    parser.add_argument("version")
    args = parser.parse_args()

    metadata = load_component_metadata(args.pack_dir / "pack.yaml")
    for manifest in sorted((args.pack_dir / "components").glob("*/component.manifest.json")):
        if stamp_manifest(manifest, args.version, metadata):
            print(f"stamped {manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
