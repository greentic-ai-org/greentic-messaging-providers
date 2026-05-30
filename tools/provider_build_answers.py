#!/usr/bin/env python3
"""Validate provider build-answer documents against committed pack metadata."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - exercised by shell users without PyYAML.
    yaml = None


ROOT = Path(__file__).resolve().parents[1]


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def load_yaml(path: Path) -> dict[str, Any]:
    if yaml is None:
        raise SystemExit("PyYAML is required for pack.yaml validation")
    value = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SystemExit(f"{path}: expected mapping")
    return value


def provider_pack_dir(provider: str) -> Path:
    name = provider
    if not name.startswith("messaging-"):
        name = f"messaging-{name}"
    pack_dir = ROOT / "packs" / name
    if not pack_dir.is_dir():
        raise SystemExit(f"unknown provider pack: {provider}")
    return pack_dir


def expected_manifest(answer: dict[str, Any]) -> dict[str, Any]:
    provider = answer["provider"]
    version = provider["version"]
    component_sources = []
    components = []
    for component in answer.get("components", []):
        item = {k: v for k, v in component.items() if k != "source"}
        component_sources.append(item)
        components.append(component["id"])

    extensions_answer = answer["extensions"]
    provider_ext = extensions_answer["provider"]
    capabilities = extensions_answer["capabilities"]

    return {
        "name": provider["id"],
        "version": version,
        "kind": provider.get("kind", "application"),
        "publisher": provider.get("publisher", "Greentic"),
        "description": provider.get("description", ""),
        "component_sources": component_sources,
        "components": components,
        "config_schema": {
            "provider_config": {
                "format": "json",
                "path": provider_ext["config_schema_ref"],
            }
        },
        "extensions": {
            "greentic.ext.capabilities.v1": {
                "kind": "greentic.ext.capabilities.v1",
                "version": version,
                "inline": {
                    "schema_version": 1,
                    "offers": [
                        {
                            "offer_id": capabilities["offer_id"],
                            "cap_id": capabilities["cap_id"],
                            "version": version,
                            "provider": {
                                "component_ref": capabilities["component_ref"],
                                "op": capabilities["op"],
                            },
                            "priority": capabilities["priority"],
                            "requires_setup": capabilities["requires_setup"],
                            "setup": {"qa_ref": capabilities["qa_ref"]},
                        }
                    ],
                },
            },
            "greentic.messaging.validators.v1": {
                "kind": "greentic.messaging.validators.v1",
                "version": version,
                "inline": {"validators": extensions_answer["validators"]},
            },
            "greentic.provider-extension.v1": {
                "kind": "greentic.provider-extension.v1",
                "version": version,
                "inline": {"providers": [provider_ext]},
            },
        },
        "messaging": answer.get("messaging", {}),
        "secret_requirements": answer.get("secret_requirements", []),
    }


def normalize(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: normalize(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


def validate(provider: str) -> None:
    pack_dir = provider_pack_dir(provider)
    answer_path = pack_dir / "build-answer.json"
    if not answer_path.exists():
        raise SystemExit(f"{answer_path}: missing build answer")
    answer = load_json(answer_path)
    if answer.get("schema_id") != "greentic-messaging-provider.build-answer":
        raise SystemExit(f"{answer_path}: unexpected schema_id")
    if answer.get("schema_version") != "1.0.0":
        raise SystemExit(f"{answer_path}: unexpected schema_version")

    manifest = load_json(pack_dir / "pack.manifest.json")
    expected = expected_manifest(answer)
    if normalize(manifest) != normalize(expected):
        print(
            f"{pack_dir}: pack.manifest.json drifted from build-answer.json",
            file=sys.stderr,
        )
        print(
            json.dumps(expected, indent=2, sort_keys=True),
            file=sys.stderr,
        )
        raise SystemExit(1)

    pack_yaml = load_yaml(pack_dir / "pack.yaml")
    provider = answer["provider"]
    for yaml_key, answer_key in [
        ("pack_id", "id"),
        ("display_name", "display_name"),
        ("version", "version"),
        ("kind", "kind"),
        ("publisher", "publisher"),
        ("description", "description"),
    ]:
        actual = pack_yaml.get(yaml_key)
        expected_value = provider.get(answer_key)
        if actual != expected_value:
            raise SystemExit(
                f"{pack_dir / 'pack.yaml'}: {yaml_key}={actual!r} "
                f"does not match build-answer provider.{answer_key}={expected_value!r}"
            )

    setup = answer.get("setup", {})
    setup_asset = setup.get("asset")
    if setup_asset and not (pack_dir / setup_asset).exists():
        raise SystemExit(f"{pack_dir}: setup asset missing: {setup_asset}")
    for asset in answer.get("assets", []):
        if not (pack_dir / asset).exists():
            raise SystemExit(f"{pack_dir}: asset missing: {asset}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("provider", help="Provider id or short name, for example email")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Validate build-answer.json against committed pack metadata",
    )
    args = parser.parse_args()
    if not args.check:
        parser.error("only --check is implemented for the initial migration")
    validate(args.provider)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
