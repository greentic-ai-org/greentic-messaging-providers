#!/usr/bin/env python3
"""Read, validate, and update provider/shared release versions."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MATRIX_PATH = ROOT / "ci" / "provider-matrix.json"
SHARED_MANIFEST = ROOT / "crates" / "provider-common" / "Cargo.toml"
LOCK_PATH = ROOT / "Cargo.lock"
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


# A generated provider's build-answer.json spells its release version
# `pack_version`; `schema_version` in the same file is unrelated and must never
# be touched, so the key is looked up by name rather than by suffix.
JSON_VERSION_KEYS = ("pack_version", "version")


def json_version_key(data: dict, path: Path) -> str:
    for key in JSON_VERSION_KEYS:
        if key in data:
            return key
    raise SystemExit(f"no {' or '.join(JSON_VERSION_KEYS)} key found in {path}")


def read_json_version(path: Path) -> str:
    data = json.loads(path.read_text())
    return data[json_version_key(data, path)]


def write_json_version(path: Path, version: str) -> None:
    data = json.loads(path.read_text())
    data[json_version_key(data, path)] = version
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def read_manifest_version(path: Path) -> str:
    """Read a declared version from a provider manifest.

    A provider's `manifests` entry is usually a Cargo.toml, but the two
    generated providers point at a `build-answer.json` instead. Reading a JSON
    file with the TOML regex matches nothing and aborts the whole run, which is
    why `validate` (with no --provider) has been unusable.
    """
    if path.suffix == ".json":
        return read_json_version(path)
    return read_toml_version(path)


def write_manifest_version(path: Path, version: str) -> None:
    if path.suffix == ".json":
        write_json_version(path, version)
        return
    write_toml_version(path, version)


def read_cargo_package(path: Path) -> tuple[str, str] | None:
    """Return (name, version) for a Cargo.toml, or None if it inherits either.

    Parsed with tomllib rather than VERSION_RE because the lock check compares
    against a package identity — a `[lib] name` or a `[[bin]] name` ahead of
    `[package]` would make a first-match regex compare the wrong crate. A
    manifest using `version.workspace = true` declares no version of its own,
    so there is nothing here for the lock to disagree with.
    """
    package = tomllib.loads(path.read_text()).get("package", {})
    name = package.get("name")
    version = package.get("version")
    if isinstance(name, str) and isinstance(version, str):
        return name, version
    return None


def load_lock_versions() -> dict[str, list[str]]:
    """Map every package name in Cargo.lock to the version(s) locked for it."""
    if not LOCK_PATH.exists():
        raise SystemExit(f"{LOCK_PATH} is missing; run `cargo update --workspace --offline`")
    locked: dict[str, list[str]] = {}
    for package in tomllib.loads(LOCK_PATH.read_text()).get("package", []):
        name = package.get("name")
        version = package.get("version")
        if name and version:
            locked.setdefault(name, []).append(version)
    return locked


def check_manifest_against_lock(path: Path, locked: dict[str, list[str]], errors: list[str]) -> None:
    """Fail when a workspace member's Cargo.toml version is not in Cargo.lock.

    This is the assertion that would have caught PR #390: the version moved in
    every declaration except the lock, so the PR was green while the tree could
    not build under `--locked`, and the publish workflow died before shipping
    anything.
    """
    if path.suffix != ".toml":
        return
    package = read_cargo_package(path)
    if package is None:
        return
    name, declared = package
    versions = locked.get(name)
    relative = path.relative_to(ROOT)
    if versions is None:
        errors.append(
            f"Cargo.lock: no entry for package '{name}' declared in {relative}; "
            "run `cargo update --workspace --offline`"
        )
    elif declared not in versions:
        errors.append(
            f"Cargo.lock: '{name}' is locked at {', '.join(versions)} but {relative} "
            f"declares {declared}; run `cargo update --workspace --offline`"
        )


def sync_cargo_lock() -> None:
    """Re-lock the workspace members after a version bump.

    `set-provider` / `set-shared` move a package version in Cargo.toml; without
    this the lock keeps the old version and every `cargo --locked` invocation
    downstream fails. `--workspace` limits the refresh to workspace members so
    registry dependencies are not churned; `--offline` keeps the helper usable
    without network. Verified to touch exactly the bumped members and nothing
    else.
    """
    if shutil.which("cargo") is None:
        raise SystemExit(
            "cargo was not found on PATH, so Cargo.lock cannot be updated to match the "
            "new version. Refusing to leave the lock stale -- install cargo and re-run, "
            "or run `cargo update --workspace --offline` yourself before committing."
        )
    print("syncing Cargo.lock (cargo update --workspace --offline)...")
    result = subprocess.run(
        ["cargo", "update", "--workspace", "--offline"],
        cwd=ROOT,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            "cargo update --workspace --offline failed, so Cargo.lock is now out of sync "
            "with the bumped Cargo.toml version(s). Fix the failure above (a cold registry "
            "cache needs a one-off `cargo fetch`) and re-run before committing."
        )


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


def validate_answer_owned_pack_version(provider_name: str, pack: str, expected: str, errors: list[str]) -> None:
    answer_path = ROOT / pack / "build-answer.json"
    if not answer_path.exists():
        return

    data = json.loads(answer_path.read_text())
    found = data.get("pack_version")
    if found != expected:
        errors.append(f"{answer_path.relative_to(ROOT)}: pack_version {found} != provider {provider_name} {expected}")

    for index, component in enumerate(data.get("runtime_components", [])):
        component_version = component.get("version")
        if component_version != expected:
            errors.append(
                f"{answer_path.relative_to(ROOT)}: runtime_components[{index}].version "
                f"{component_version} != provider {provider_name} {expected}"
            )


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
    locked = load_lock_versions()
    for name in names:
        provider = matrix["providers"][name]
        expected = provider["version"]
        for manifest in provider["manifests"]:
            manifest_path = ROOT / manifest
            found = read_manifest_version(manifest_path)
            if found != expected:
                errors.append(f"{manifest}: {found} != provider {name} {expected}")
            check_manifest_against_lock(manifest_path, locked, errors)
        pack_yaml, pack_manifest = pack_paths(provider)
        if pack_yaml.exists():
            found = read_pack_yaml_version(pack_yaml)
            if found != expected:
                errors.append(f"{pack_yaml.relative_to(ROOT)}: {found} != provider {name} {expected}")
        if pack_manifest.exists():
            found = json.loads(pack_manifest.read_text()).get("version")
            if found != expected:
                errors.append(f"{pack_manifest.relative_to(ROOT)}: {found} != provider {name} {expected}")
        validate_answer_owned_pack_version(name, provider["pack"], expected, errors)
    # The shared crate is bumped by `set-shared`, which had the same lock gap.
    check_manifest_against_lock(SHARED_MANIFEST, locked, errors)
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
        write_manifest_version(ROOT / manifest, args.version)
    pack_yaml, pack_manifest = pack_paths(provider)
    if pack_yaml.exists():
        write_pack_yaml_version(pack_yaml, args.version, old_version)
    if pack_manifest.exists():
        update_manifest_json_versions(pack_manifest, args.version)
    update_answer_owned_pack_version(provider["pack"], args.version, old_version)
    write_matrix(matrix)
    sync_cargo_lock()
    print(f"set {name} to {args.version}")
    return 0


def cmd_set_shared(args: argparse.Namespace) -> int:
    write_toml_version(SHARED_MANIFEST, args.version)
    sync_cargo_lock()
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
