#!/usr/bin/env python3
"""Normalize pack.yaml component declarations.

Pack tooling can discover both legacy nested component artifacts
(`components/<id>/component.wasm`) and canonical flat artifacts
(`components/<id>.wasm`). Keep one declaration per component id so generated
manifests do not advertise duplicate components.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import yaml


def component_score(component: dict[str, Any]) -> tuple[int, int, int]:
    comp_id = str(component.get("id") or "")
    wasm = str(component.get("wasm") or "")
    canonical = f"components/{comp_id}.wasm"

    if wasm == canonical:
        path_score = 3
    elif not wasm.endswith("/component.wasm"):
        path_score = 2
    else:
        path_score = 1

    # Prefer entries that already carry full metadata if paths tie.
    metadata_score = sum(
        1
        for key in ("world", "supports", "profiles", "capabilities", "oci", "manifest")
        if key in component
    )
    return (path_score, metadata_score, len(wasm))


def normalize_components(components: Any, version: str | None = None) -> tuple[Any, bool]:
    if not isinstance(components, list):
        return components, False

    selected: dict[str, tuple[int, dict[str, Any]]] = {}
    anonymous: list[tuple[int, Any]] = []
    changed = False

    for index, component in enumerate(components):
        if not isinstance(component, dict):
            anonymous.append((index, component))
            continue
        comp_id = component.get("id")
        if not comp_id:
            anonymous.append((index, component))
            continue

        previous = selected.get(str(comp_id))
        if version and component.get("version") != version:
            component = dict(component)
            component["version"] = version
            changed = True

        if previous is None:
            selected[str(comp_id)] = (index, component)
            continue

        changed = True
        _, previous_component = previous
        if component_score(component) > component_score(previous_component):
            selected[str(comp_id)] = (index, component)

    if not changed:
        return components, False

    retained_by_index = {index: component for index, component in selected.values()}
    retained_by_index.update(dict(anonymous))
    normalized = [retained_by_index[index] for index in sorted(retained_by_index)]
    return normalized, True


def normalize_pack(path: Path) -> bool:
    data = yaml.safe_load(path.read_text()) or {}
    if not isinstance(data, dict):
        return False
    version = str(data.get("version") or "") or None
    components, changed = normalize_components(data.get("components"), version)
    if not changed:
        return False
    data["components"] = components
    path.write_text(yaml.safe_dump(data, sort_keys=False))
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pack_yaml", type=Path, nargs="+")
    args = parser.parse_args()

    for path in args.pack_yaml:
        if path.exists() and normalize_pack(path):
            print(f"normalized duplicate component ids in {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
