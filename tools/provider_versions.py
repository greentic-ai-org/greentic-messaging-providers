#!/usr/bin/env python3
"""Read, validate, and update provider/shared release versions."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MATRIX_PATH = ROOT / "ci" / "provider-matrix.json"
SHARED_MANIFEST = ROOT / "crates" / "provider-common" / "Cargo.toml"
VERSION_RE = re.compile(r'^(version\s*=\s*)"([^"]+)"', re.MULTILINE)
YAML_VERSION_RE = re.compile(r"^(\s*version:\s*)([^\n#]+)", re.MULTILINE)
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")


def load_matrix() -> dict:
    return json.loads(MATRIX_PATH.read_text())


def write_matrix(matrix: dict) -> None:
    MATRIX_PATH.write_text(json.dumps(matrix, indent=2) + "\n", encoding="utf-8")


def read_toml_version(path: Path) -> str:
    match = VERSION_RE.search(path.read_text())
    if not match:
        raise SystemExit(f"no package version found in {path}")
    return match.group(2)


def write_toml_version(path: Path, version: str) -> None:
    text = path.read_text()
    updated, count = VERSION_RE.subn(rf'\g<1>"{version}"', text, count=1)
    if count != 1:
        raise SystemExit(f"could not update version in {path}")
    path.write_text(updated, encoding="utf-8")


def read_pack_yaml_version(path: Path) -> str:
    match = YAML_VERSION_RE.search(path.read_text())
    if not match:
        raise SystemExit(f"no pack version found in {path}")
    return match.group(2).strip().strip('"\'')


def write_pack_yaml_version(path: Path, version: str, old_version: str | None = None) -> None:
    text = path.read_text()
    root_version = read_pack_yaml_version(path)
    updated, count = YAML_VERSION_RE.subn(rf"\g<1>{version}", text, count=1)
    if count < 1:
        raise SystemExit(f"could not update pack version in {path}")
    lines = []
    replace_versions = {value for value in (old_version, root_version, version) if value and SEMVER_RE.match(value)}
    for line in updated.splitlines():
        match = YAML_VERSION_RE.match(line)
        current = match.group(2).strip().strip('"\'') if match else ""
        if match and current in replace_versions:
            line = f"{match.group(1)}{version}"
        lines.append(line)
    updated = "\n".join(lines) + "\n"
    path.write_text(updated, encoding="utf-8")


def pack_paths(provider: dict) -> tuple[Path, Path]:
    pack_dir = ROOT / "packs" / provider["pack"]
    return pack_dir / "pack.yaml", pack_dir / "pack.manifest.json"


def update_manifest_json_versions(path: Path, version: str) -> None:
    data = json.loads(path.read_text())
    old_version = data.get("version")

    def walk(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == "version" and item == old_version:
                    value[key] = version
                else:
                    walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    data["version"] = version
    walk(data)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def provider_version(matrix: dict, name: str) -> str:
    provider = matrix["providers"][name]
    return provider["version"]


def resolve_provider_name(raw: str, matrix: dict) -> str:
    value = raw.strip().lower()
    if value in matrix["providers"]:
        return value
    stripped = value.removeprefix("messaging-")
    if stripped in matrix["providers"]:
        return stripped
    raise SystemExit(f"unknown provider: {raw}")


def update_answer_owned_pack_version(pack: str, version: str, old_version: str) -> None:
    answer_path = ROOT / pack / "build-answer.json"
    if not answer_path.exists():
        return

    data = json.loads(answer_path.read_text())

    def walk(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key in {"pack_version", "version"} and item == old_version:
                    value[key] = version
                elif isinstance(item, str) and old_version in item:
                    value[key] = item.replace(old_version, version)
                else:
                    walk(item)
        elif isinstance(value, list):
            for index, item in enumerate(value):
                if isinstance(item, str) and old_version in item:
                    value[index] = item.replace(old_version, version)
                else:
                    walk(item)

    walk(data)
    answer_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def cmd_list(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    result = {
        "shared_crate": {
            "name": "greentic-messaging-provider-common",
            "version": read_toml_version(SHARED_MANIFEST),
        },
        "providers": {
            name: {
                "version": provider["version"],
                "pack": provider["pack"],
                "ghcr_target": provider["ghcr_target"],
            }
            for name, provider in sorted(matrix["providers"].items())
        },
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


def cmd_shared(args: argparse.Namespace) -> int:
    print(read_toml_version(SHARED_MANIFEST))
    return 0


def cmd_provider(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    name = resolve_provider_name(args.provider, matrix)
    print(provider_version(matrix, name))
    return 0


def cmd_validate(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    names = sorted(matrix["providers"].keys()) if args.provider == "all" else [args.provider]
    errors: list[str] = []
    for name in names:
        provider = matrix["providers"][name]
        expected = provider["version"]
        for manifest in provider["manifests"]:
            found = read_toml_version(ROOT / manifest)
            if found != expected:
                errors.append(f"{manifest}: {found} != provider {name} {expected}")
        pack_yaml, pack_manifest = pack_paths(provider)
        if pack_yaml.exists():
            found = read_pack_yaml_version(pack_yaml)
            if found != expected:
                errors.append(f"{pack_yaml.relative_to(ROOT)}: {found} != provider {name} {expected}")
        if pack_manifest.exists():
            found = json.loads(pack_manifest.read_text()).get("version")
            if found != expected:
                errors.append(f"{pack_manifest.relative_to(ROOT)}: {found} != provider {name} {expected}")
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(f"validated {len(names)} provider version(s)")
    return 0


def cmd_set_provider(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    name = resolve_provider_name(args.provider, matrix)
    provider = matrix["providers"][name]
    old_version = provider["version"]
    provider["version"] = args.version
    for manifest in provider["manifests"]:
        write_toml_version(ROOT / manifest, args.version)
    pack_yaml, pack_manifest = pack_paths(provider)
    if pack_yaml.exists():
        write_pack_yaml_version(pack_yaml, args.version, old_version)
    if pack_manifest.exists():
        update_manifest_json_versions(pack_manifest, args.version)
    update_answer_owned_pack_version(provider["pack"], args.version, old_version)
    write_matrix(matrix)
    print(f"set {name} to {args.version}")
    return 0


def cmd_set_shared(args: argparse.Namespace) -> int:
    write_toml_version(SHARED_MANIFEST, args.version)
    print(f"set shared crate to {args.version}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("list").set_defaults(func=cmd_list)
    subparsers.add_parser("shared").set_defaults(func=cmd_shared)

    provider = subparsers.add_parser("provider")
    provider.add_argument("provider")
    provider.set_defaults(func=cmd_provider)

    validate = subparsers.add_parser("validate")
    validate.add_argument("--provider", default="all")
    validate.set_defaults(func=cmd_validate)

    set_provider = subparsers.add_parser("set-provider")
    set_provider.add_argument("provider")
    set_provider.add_argument("version")
    set_provider.set_defaults(func=cmd_set_provider)

    set_shared = subparsers.add_parser("set-shared")
    set_shared.add_argument("version")
    set_shared.set_defaults(func=cmd_set_shared)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
