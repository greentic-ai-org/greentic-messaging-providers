#!/usr/bin/env python3
"""Validate every WebChat skin against schemas/webchat/skin.schema.json.

Checks the manifest shape, that `tenant` matches the directory name, and that
every path the manifest references resolves to a file that exists.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

try:
    from jsonschema import Draft7Validator
except ImportError:
    print("validate_skins: jsonschema not installed; run `pip install jsonschema`", file=sys.stderr)
    raise SystemExit(1)

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = json.loads((ROOT / "schemas/webchat/skin.schema.json").read_text(encoding="utf-8"))
SKIN_ROOTS = [
    ROOT / "packs/messaging-webchat-gui/assets/webchat-gui/skins",
]

# Keys whose string values are paths that must resolve to a real file.
PATH_KEYS = [
    ("brand", "favicon"),
    ("brand", "logo"),
    ("webchat", "styleOptions"),
    ("webchat", "adaptiveCardsHostConfig"),
    ("fullpage", "index"),
    ("fullpage", "css"),
    ("hooks", "script"),
]


def resolve(skins_root: Path, skin_dir: Path, ref: str) -> Path:
    """Resolve a manifest path reference to a file on disk.

    Refs are root-absolute (`/skins/<name>/...`), resolved against the SPA root;
    the SPA's own base-path resolver prepends the mount point at runtime.
    """
    if ref.startswith("/skins/"):
        return skins_root / ref[len("/skins/"):]
    return (skin_dir / ref).resolve()


def check_skin(skins_root: Path, skin_dir: Path, errors: list[str]) -> None:
    manifest_path = skin_dir / "skin.json"
    if not manifest_path.is_file():
        errors.append(f"{skin_dir}: no skin.json")
        return
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        errors.append(f"{manifest_path}: invalid JSON: {err}")
        return

    for problem in Draft7Validator(SCHEMA).iter_errors(manifest):
        location = "/".join(str(p) for p in problem.absolute_path) or "<root>"
        errors.append(f"{manifest_path}: {location}: {problem.message}")

    if manifest.get("tenant") != skin_dir.name:
        errors.append(
            f"{manifest_path}: tenant {manifest.get('tenant')!r} does not match "
            f"directory {skin_dir.name!r}"
        )

    expected_prefix = f"/skins/{skin_dir.name}/"
    for section, key in PATH_KEYS:
        ref = manifest.get(section, {}).get(key)
        if isinstance(ref, str) and not ref.startswith(expected_prefix):
            errors.append(
                f"{manifest_path}: {section}.{key} -> {ref!r} must start with "
                f"{expected_prefix!r} (root-absolute, pointing at this skin's own "
                "directory)"
            )

    for section, key in PATH_KEYS:
        ref = manifest.get(section, {}).get(key)
        if not isinstance(ref, str):
            continue
        target = resolve(skins_root, skin_dir, ref)
        if not target.is_file():
            errors.append(f"{manifest_path}: {section}.{key} -> {ref} does not exist")


def main() -> int:
    errors: list[str] = []
    checked = 0
    for skins_root in SKIN_ROOTS:
        if not skins_root.is_dir():
            continue
        for skin_dir in sorted(p for p in skins_root.iterdir() if p.is_dir()):
            check_skin(skins_root, skin_dir, errors)
            checked += 1

    if errors:
        for error in errors:
            print(f"skin validation: {error}", file=sys.stderr)
        return 1
    print(f"skin validation: {checked} skins OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
