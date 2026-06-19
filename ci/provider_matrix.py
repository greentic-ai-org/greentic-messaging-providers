#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[1]
MATRIX_PATH = ROOT_DIR / "ci" / "provider-matrix.json"
VERSION_RE = re.compile(r'^version\s*=\s*"([^"]+)"\s*$', re.MULTILINE)

DOC_PATH_PREFIXES = (
    ".codex/",
    "docs/",
)
DOC_PATHS = {
    "README.md",
    "CHANGELOG.md",
    "LICENSE",
}
TOOLING_PATH_PREFIXES = (
    ".github/",
    "ci/",
    "tools/",
)


def load_matrix() -> dict:
    return json.loads(MATRIX_PATH.read_text())


def normalize_provider(raw: str, matrix: dict) -> str:
    value = raw.strip().lower()
    if value in matrix["providers"] and value != "teams":
        return value
    for name, provider in matrix["providers"].items():
        if value == provider.get("pack", "").lower():
            return name
    # The public Teams provider is the Bot Framework-backed messaging-teams pack.
    # Keep the legacy Graph provider reachable by its explicit pack id above.
    if value == "teams" and "messaging-teams" in matrix["providers"]:
        return "messaging-teams"
    if value in matrix["providers"]:
        return value
    if value.startswith("messaging-"):
        value = value[len("messaging-") :]
    if value in matrix["providers"]:
        return value
    raise SystemExit(
        f"Unknown provider '{raw}'. Expected one of: "
        + ", ".join(sorted(matrix["providers"].keys()))
    )


def matches_path(path: str, candidate: str) -> bool:
    if candidate.endswith("/"):
        return path.startswith(candidate)
    return path == candidate


def read_manifest_version(manifest: str) -> str | None:
    path = ROOT_DIR / manifest
    if not path.exists():
        return None
    match = VERSION_RE.search(path.read_text())
    if match:
        return match.group(1)
    return None


def provider_version(provider: dict) -> str:
    if provider.get("version"):
        return provider["version"]
    for manifest in provider.get("manifests", []):
        version = read_manifest_version(manifest)
        if version:
            return version
    return "unknown"


def provider_summary(name: str, provider: dict) -> dict:
    pack = provider["pack"]
    return {
        "provider": name,
        "pack": pack,
        "version": provider_version(provider),
        "ghcr_target": provider.get("ghcr_target", f"ghcr.io/greenticai/{pack}"),
        "shared_crate_dependency": provider.get("shared_crate_dependency"),
        "components": provider["components"],
        "manifests": provider["manifests"],
        "paths": provider["paths"],
        "e2e": provider.get("e2e"),
    }


def is_docs_path(path: str) -> bool:
    return path in DOC_PATHS or any(path.startswith(prefix) for prefix in DOC_PATH_PREFIXES)


def is_tooling_path(path: str) -> bool:
    return any(path.startswith(prefix) for prefix in TOOLING_PATH_PREFIXES)


def git_ref_exists(ref: str) -> bool:
    return (
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}"],
            cwd=ROOT_DIR,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        == 0
    )


def diff_name_only(base: str, head: str) -> list[str]:
    cmd = ["git", "diff", "--name-only", f"{base}..{head}"]
    # Accepted risk: CI invokes a fixed git subcommand with argument vector, not a shell.
    # foxguard: ignore[py/no-command-injection]
    output = subprocess.check_output(cmd, cwd=ROOT_DIR, text=True)
    return [line.strip() for line in output.splitlines() if line.strip()]


def detect_changed_files(base: str, head: str) -> tuple[list[str], str | None]:
    try:
        return diff_name_only(base, head), None
    except subprocess.CalledProcessError:
        if not git_ref_exists(head) or not git_ref_exists("origin/main"):
            raise

        fallback_base = subprocess.check_output(
            ["git", "merge-base", head, "origin/main"],
            cwd=ROOT_DIR,
            text=True,
        ).strip()
        if not fallback_base:
            raise

        return (
            diff_name_only(fallback_base, head),
            f"base revision {base} unavailable; using merge-base against origin/main",
        )


def resolve_provider(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    provider_name = normalize_provider(args.provider, matrix)
    provider = matrix["providers"][provider_name]
    result = provider_summary(provider_name, provider)
    print(json.dumps(result))
    return 0


def list_providers(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    providers = [
        provider_summary(name, provider)
        for name, provider in sorted(matrix["providers"].items())
    ]
    if args.format == "text":
        for provider in providers:
            print(
                "{provider}\t{version}\t{pack}\t{ghcr_target}\t{components}".format(
                    provider=provider["provider"],
                    version=provider["version"],
                    pack=provider["pack"],
                    ghcr_target=provider["ghcr_target"],
                    components=",".join(provider["components"]),
                )
            )
    else:
        print(json.dumps({"providers": providers}))
    return 0


def select_provider_names(raw: str, matrix: dict) -> list[str]:
    value = (raw or "all").strip()
    if not value or value.lower() == "all":
        return sorted(matrix["providers"].keys())
    names = []
    for item in value.split(","):
        item = item.strip()
        if not item:
            continue
        names.append(normalize_provider(item, matrix))
    return sorted(dict.fromkeys(names))


def e2e_summary(name: str, provider: dict) -> dict:
    summary = provider_summary(name, provider)
    e2e = summary.get("e2e") or {}
    return {
        "provider": name,
        "pack": summary["pack"],
        "version": summary["version"],
        "ghcr_target": summary["ghcr_target"],
        "components": summary["components"],
        "required_secrets": e2e.get("required_secrets", []),
        "optional_secrets": e2e.get("optional_secrets", []),
        "fixture_path": e2e.get("fixture_path"),
        "test_command": e2e.get("test_command"),
        "external_service": e2e.get("external_service"),
        "operation": e2e.get("operation"),
        "readback": e2e.get("readback"),
    }


def list_e2e_providers(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    names = select_provider_names(args.provider, matrix)
    providers = [
        e2e_summary(name, matrix["providers"][name])
        for name in names
        if matrix["providers"][name].get("e2e") is not None
    ]
    print(json.dumps({"providers": providers, "provider_names": [p["provider"] for p in providers]}))
    return 0


def resolve_e2e_provider(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    provider_name = normalize_provider(args.provider, matrix)
    provider = matrix["providers"][provider_name]
    if provider.get("e2e") is None:
        raise SystemExit(f"provider '{provider_name}' has no e2e metadata")
    print(json.dumps(e2e_summary(provider_name, provider)))
    return 0


def empty_result(
    changed_files: list[str],
    reason: str,
    change_class: str,
    *,
    docs_only: bool = False,
    tooling_only: bool = False,
) -> dict:
    return {
        "build_all": False,
        "reason": reason,
        "change_class": change_class,
        "classification": change_class,
        "changed_files": changed_files,
        "affected_providers": [],
        "affected_components": [],
        "affected_packs": [],
        "affected_manifests": [],
        "affected_versions": {},
        "affected_ghcr_targets": {},
        "shared_changed": False,
        "shared_crate_changed": False,
        "provider_changed": False,
        "tooling_changed": tooling_only,
        "docs_only": docs_only,
        "tooling_only": tooling_only,
        "provider_reasons": {},
    }


def build_all_result(matrix: dict, changed_files: list[str], reason: str, change_class: str = "shared") -> dict:
    affected_components = list(
        dict.fromkeys(
            component
            for provider in matrix["providers"].values()
            for component in provider["components"]
        )
    )
    affected_packs = list(
        dict.fromkeys(provider["pack"] for provider in matrix["providers"].values())
    )
    return {
        "build_all": True,
        "reason": reason,
        "change_class": change_class,
        "classification": change_class,
        "changed_files": changed_files,
        "affected_providers": sorted(matrix["providers"].keys()),
        "affected_components": affected_components,
        "affected_packs": affected_packs,
        "affected_versions": {
            name: provider_version(provider)
            for name, provider in sorted(matrix["providers"].items())
        },
        "affected_ghcr_targets": {
            name: provider.get("ghcr_target", f"ghcr.io/greenticai/{provider['pack']}")
            for name, provider in sorted(matrix["providers"].items())
        },
        "affected_manifests": sorted(
            {
                manifest
                for provider in matrix["providers"].values()
                for manifest in provider["manifests"]
            }
        ),
        "shared_changed": change_class == "shared",
        "shared_crate_changed": change_class == "shared",
        "provider_changed": True,
        "tooling_changed": change_class == "tooling",
        "docs_only": False,
        "tooling_only": False,
        "provider_reasons": {
            name: "selected by shared-crate fanout" if change_class == "shared" else f"selected by {change_class} fallback"
            for name in sorted(matrix["providers"].keys())
        },
    }


def detect_changes(args: argparse.Namespace) -> int:
    matrix = load_matrix()
    changed_files, fallback_reason = detect_changed_files(args.base, args.head)
    if not changed_files:
        print(json.dumps(build_all_result(matrix, changed_files, "no changed files detected")))
        return 0

    if all(is_docs_path(path) for path in changed_files):
        print(json.dumps(empty_result(changed_files, "docs-only changes", "docs", docs_only=True)))
        return 0

    if all(is_tooling_path(path) for path in changed_files):
        print(
            json.dumps(
                empty_result(
                    changed_files,
                    "tooling/CI-only changes",
                    "tooling",
                    tooling_only=True,
                )
            )
        )
        return 0

    shared_paths = matrix["shared_paths"]
    owners: dict[str, set[str]] = {provider: set() for provider in matrix["providers"]}

    for path in changed_files:
        if any(matches_path(path, candidate) for candidate in shared_paths):
            print(json.dumps(build_all_result(matrix, changed_files, f"shared path changed: {path}")))
            return 0

        matched = [
            provider_name
            for provider_name, provider in matrix["providers"].items()
            if any(matches_path(path, candidate) for candidate in provider["paths"])
        ]
        if not matched:
            print(json.dumps(build_all_result(matrix, changed_files, f"unmapped path changed: {path}", "unmapped")))
            return 0
        if len(matched) > 1:
            print(json.dumps(build_all_result(matrix, changed_files, f"multi-provider path changed: {path}", "provider")))
            return 0
        owners[matched[0]].add(path)

    affected_providers = sorted(name for name, paths in owners.items() if paths)
    affected_components = []
    affected_packs = []
    affected_manifests = []
    for provider_name in affected_providers:
        provider = matrix["providers"][provider_name]
        affected_components.extend(provider["components"])
        affected_packs.append(provider["pack"])
        affected_manifests.extend(provider["manifests"])

    result = {
        "build_all": False,
        "reason": fallback_reason or "provider-scoped changes only",
        "change_class": "provider",
        "classification": "provider",
        "changed_files": changed_files,
        "affected_providers": affected_providers,
        "affected_components": list(dict.fromkeys(affected_components)),
        "affected_packs": list(dict.fromkeys(affected_packs)),
        "affected_manifests": list(dict.fromkeys(affected_manifests)),
        "affected_versions": {
            name: provider_version(matrix["providers"][name]) for name in affected_providers
        },
        "affected_ghcr_targets": {
            name: matrix["providers"][name].get(
                "ghcr_target", f"ghcr.io/greenticai/{matrix['providers'][name]['pack']}"
            )
            for name in affected_providers
        },
        "shared_changed": False,
        "shared_crate_changed": False,
        "provider_changed": bool(affected_providers),
        "tooling_changed": False,
        "docs_only": False,
        "tooling_only": False,
        "provider_reasons": {
            name: (
                "selected by provider-owned path"
                if name in affected_providers
                else "skipped; no provider-owned path changed"
            )
            for name in sorted(matrix["providers"].keys())
        },
    }
    print(json.dumps(result))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    resolve_parser = subparsers.add_parser("resolve-provider")
    resolve_parser.add_argument("provider")
    resolve_parser.set_defaults(func=resolve_provider)

    list_parser = subparsers.add_parser("list-providers")
    list_parser.add_argument("--format", choices=["json", "text"], default="json")
    list_parser.set_defaults(func=list_providers)

    list_e2e_parser = subparsers.add_parser("list-e2e-providers")
    list_e2e_parser.add_argument("--provider", default="all")
    list_e2e_parser.set_defaults(func=list_e2e_providers)

    resolve_e2e_parser = subparsers.add_parser("resolve-e2e-provider")
    resolve_e2e_parser.add_argument("provider")
    resolve_e2e_parser.set_defaults(func=resolve_e2e_provider)

    detect_parser = subparsers.add_parser("detect-changes")
    detect_parser.add_argument("--base", required=True)
    detect_parser.add_argument("--head", required=True)
    detect_parser.set_defaults(func=detect_changes)

    affected_parser = subparsers.add_parser("affected")
    affected_parser.add_argument("--base", required=True)
    affected_parser.add_argument("--head", required=True)
    affected_parser.set_defaults(func=detect_changes)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
