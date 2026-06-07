#!/usr/bin/env python3
"""
Validate that packed gtpack manifests contain the canonical provider extension key.
"""

from __future__ import annotations

import argparse
import json
import sys
import zipfile
from pathlib import Path
from typing import Any, Dict, Tuple

PROVIDER_EXTENSION_ID = "greentic.provider-extension.v1"

GENERATED_SECRET_REQUIREMENTS = {
    "messaging-webex": ("webex_webhook_secret", "WEBEX_WEBHOOK_SECRET"),
    "messaging-webchat-gui": ("jwt_signing_key", "JWT_SIGNING_KEY"),
    "messaging-webchat": ("jwt_signing_key", "JWT_SIGNING_KEY"),
}


class CBORDecoder:
    """
    Minimal CBOR decoder for the pack manifest structure.
    Supports the types used by pack manifests (maps, arrays, text, ints, bools, floats).
    """

    def __init__(self, data: bytes):
        self.data = data
        self.pos = 0

    def read(self, n: int) -> bytes:
        if self.pos + n > len(self.data):
            raise ValueError("truncated CBOR input")
        chunk = self.data[self.pos : self.pos + n]
        self.pos += n
        return chunk

    def decode_uint(self, addl: int) -> int:
        if addl < 24:
            return addl
        if addl == 24:
            return self.read(1)[0]
        if addl == 25:
            return int.from_bytes(self.read(2), "big")
        if addl == 26:
            return int.from_bytes(self.read(4), "big")
        if addl == 27:
            return int.from_bytes(self.read(8), "big")
        raise ValueError(f"unsupported additional length: {addl}")

    def decode(self) -> Any:
        if self.pos >= len(self.data):
            raise EOFError("unexpected end of CBOR input")
        initial = self.read(1)[0]
        major = initial >> 5
        addl = initial & 0x1F

        if major == 0:  # unsigned int
            return self.decode_uint(addl)
        if major == 1:  # negative int
            return -1 - self.decode_uint(addl)
        if major == 2:  # bytes
            length = self.decode_uint(addl)
            return self.read(length)
        if major == 3:  # text
            length = self.decode_uint(addl)
            return self.read(length).decode("utf-8")
        if major == 4:  # array
            items = []
            if addl == 31:
                while True:
                    if self.data[self.pos] == 0xFF:
                        self.pos += 1
                        break
                    items.append(self.decode())
            else:
                length = self.decode_uint(addl)
                for _ in range(length):
                    items.append(self.decode())
            return items
        if major == 5:  # map
            obj: Dict[Any, Any] = {}
            if addl == 31:
                while True:
                    if self.data[self.pos] == 0xFF:
                        self.pos += 1
                        break
                    key = self.decode()
                    obj[key] = self.decode()
            else:
                length = self.decode_uint(addl)
                for _ in range(length):
                    key = self.decode()
                    obj[key] = self.decode()
            return obj
        if major == 6:  # tag (ignored)
            _ = self.decode_uint(addl)
            return self.decode()
        if major == 7:  # floats/simple
            if addl == 20:
                return False
            if addl == 21:
                return True
            if addl == 22 or addl == 23:
                return None
            if addl == 26:
                import struct

                return struct.unpack(">f", self.read(4))[0]
            if addl == 27:
                import struct

                return struct.unpack(">d", self.read(8))[0]
        raise ValueError(f"unsupported CBOR major/additional: {major}/{addl}")


def load_manifest_from_pack(path: Path) -> Dict[str, Any]:
    with zipfile.ZipFile(path, "r") as zf:
        try:
            data = zf.read("manifest.cbor")
        except KeyError as exc:
            raise ValueError(f"{path} missing manifest.cbor") from exc
    decoder = CBORDecoder(data)
    manifest = decoder.decode()
    if not isinstance(manifest, dict):
        raise ValueError(f"{path} manifest is not a CBOR map")
    return manifest


def validate_pack(path: Path) -> None:
    if not path.stem.startswith("messaging-"):
        return

    manifest = load_manifest_from_pack(path)
    extensions = manifest.get("extensions")
    if not isinstance(extensions, dict):
        raise ValueError(f"{path} manifest has no extensions map")

    ext = extensions.get(PROVIDER_EXTENSION_ID)
    if ext is None:
        keys = ", ".join(sorted(k for k in extensions.keys() if isinstance(k, str)))
        raise ValueError(
            f"{path} missing provider extension {PROVIDER_EXTENSION_ID} (keys: {keys})"
        )

    if isinstance(ext, dict):
        kind = ext.get("kind")
        if kind != PROVIDER_EXTENSION_ID:
            raise ValueError(
                f"{path} provider extension kind={kind!r}, expected {PROVIDER_EXTENSION_ID!r}"
            )

    if path.stem == "messaging-teams-graph":
        validate_teams_subscription_desired_state(path, manifest)

    if path.stem in GENERATED_SECRET_REQUIREMENTS:
        secret_name, alias = GENERATED_SECRET_REQUIREMENTS[path.stem]
        validate_generated_secret_requirement(path, manifest, secret_name, alias)


def validate_generated_secret_requirement(
    path: Path, manifest: Dict[str, Any], secret_name: str, alias: str
) -> None:
    generated_extension = (
        (manifest.get("extensions") or {})
        .get("greentic.generated-secrets.v1", {})
        .get("inline", {})
        .get("secrets")
    )
    if isinstance(generated_extension, list):
        for candidate in generated_extension:
            if isinstance(candidate, dict) and candidate.get("key") == secret_name:
                validate_generated_secret_policy(path, candidate, secret_name, alias)
                return

    requirements = manifest.get("secret_requirements")
    if not isinstance(requirements, list):
        raise ValueError(f"{path} manifest missing secret_requirements list")
    requirement = None
    for candidate in requirements:
        if isinstance(candidate, dict) and candidate.get("name") == secret_name:
            requirement = candidate
            break
    if requirement is None:
        raise ValueError(f"{path} missing generated secret requirement {secret_name}")

    validate_generated_secret_policy(path, requirement, secret_name, alias)


def validate_generated_secret_policy(
    path: Path, requirement: Dict[str, Any], secret_name: str, alias: str
) -> None:
    aliases = requirement.get("aliases")
    if not isinstance(aliases, list) or alias not in aliases:
        raise ValueError(f"{path} {secret_name} missing alias {alias}")
    if requirement.get("required") is not True:
        raise ValueError(f"{path} {secret_name} must be required")
    scope_value = requirement.get("scope")
    if isinstance(scope_value, dict):
        if scope_value.get("level") != "tenant" or scope_value.get("team") != "_":
            raise ValueError(f"{path} {secret_name} must use tenant-wide team=_ scope")
    elif scope_value != "tenant":
        raise ValueError(f"{path} {secret_name} must use tenant requirement scope")

    generated = requirement.get("generated")
    if not isinstance(generated, dict):
        generated = requirement
    expected = {
        "policy": "random",
        "length": 20,
        "encoding": "raw_text",
        "regenerate_if_present": False,
    }
    for key, value in expected.items():
        if generated.get(key) != value:
            raise ValueError(
                f"{path} {secret_name} generated.{key}={generated.get(key)!r}, expected {value!r}"
            )
    scope = generated.get("scope")
    if not isinstance(scope, dict) or scope.get("level") != "tenant" or scope.get("team") != "_":
        raise ValueError(f"{path} {secret_name} generated scope must be tenant-wide team=_")


def validate_teams_subscription_desired_state(path: Path, manifest: Dict[str, Any]) -> None:
    subscriptions = (
        (manifest.get("extensions") or {})
        .get("messaging.subscriptions.v1", {})
        .get("inline")
    )
    if not isinstance(subscriptions, dict):
        raise ValueError(f"{path} missing Teams messaging.subscriptions.v1 inline metadata")

    desired_state = subscriptions.get("desired_state")
    if not isinstance(desired_state, dict):
        raise ValueError(f"{path} Teams subscriptions metadata missing desired_state")

    if desired_state.get("output_key") != "desired_subscriptions":
        raise ValueError(
            f"{path} Teams desired_state output_key={desired_state.get('output_key')!r}, "
            "expected 'desired_subscriptions'"
        )

    source_keys = desired_state.get("source_keys")
    if not isinstance(source_keys, list):
        raise ValueError(f"{path} Teams desired_state missing source_keys list")
    for key in ("team_id", "channel_id"):
        if key not in source_keys:
            raise ValueError(f"{path} Teams desired_state source_keys missing {key}")

    templates = desired_state.get("templates")
    if not isinstance(templates, list):
        raise ValueError(f"{path} Teams desired_state missing templates list")

    channel_template = None
    for template in templates:
        if not isinstance(template, dict):
            continue
        if (
            template.get("resource_template")
            == "/teams/{team_id}/channels/{channel_id}/messages"
        ):
            channel_template = template
            break
    if channel_template is None:
        raise ValueError(
            f"{path} Teams desired_state templates missing channel message resource template"
        )

    when_all = channel_template.get("when_all")
    if not isinstance(when_all, list) or "team_id" not in when_all or "channel_id" not in when_all:
        raise ValueError(
            f"{path} Teams channel subscription template must require team_id and channel_id"
        )

    component_config = subscriptions.get("component_config")
    if not isinstance(component_config, dict):
        raise ValueError(f"{path} Teams subscriptions metadata missing component_config")
    include = component_config.get("include")
    if not isinstance(include, list):
        raise ValueError(f"{path} Teams component_config missing include list")
    for key in ("team_id", "channel_id", "chat_id"):
        if key in include:
            raise ValueError(
                f"{path} Teams component_config must not pass desired-state key {key}"
            )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate provider extension key in gtpack manifests."
    )
    parser.add_argument("packs", nargs="+", type=Path, help="Paths to .gtpack files")
    args = parser.parse_args()

    errors = []
    for pack_path in args.packs:
        try:
            validate_pack(pack_path)
        except Exception as exc:  # pylint: disable=broad-except
            errors.append(str(exc))

    if errors:
        for err in errors:
            sys.stderr.write(err + "\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
