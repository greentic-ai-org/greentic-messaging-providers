#!/usr/bin/env python3
"""Provider nightly e2e runner with sanitized result output."""

from __future__ import annotations

import argparse
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MATRIX = json.loads((ROOT / "ci" / "provider-matrix.json").read_text())


def metadata(provider: str) -> dict:
    name = provider.strip().lower().removeprefix("messaging-")
    if name not in MATRIX["providers"]:
        raise SystemExit(f"unknown provider: {provider}")
    info = MATRIX["providers"][name]
    if "e2e" not in info:
        raise SystemExit(f"provider {name} has no e2e metadata")
    return {"provider": name, **info, "e2e": info["e2e"]}


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def result_base(info: dict, args: argparse.Namespace, status: str, reason: str | None = None) -> dict:
    e2e = info["e2e"]
    result = {
        "provider": info["provider"],
        "pack": info["pack"],
        "version": info["version"],
        "gtpack": args.gtpack,
        "gtpack_source": args.gtpack_source,
        "correlation_id": args.correlation_id,
        "operation": e2e.get("operation"),
        "external_service": e2e.get("external_service"),
        "readback": e2e.get("readback"),
        "result": status,
    }
    if reason:
        result["reason"] = reason
    return result


def missing_required_secrets(info: dict) -> list[str]:
    return [
        name
        for name in info["e2e"].get("required_secrets", [])
        if not os.environ.get(name)
    ]


def ensure_gtpack_exists(args: argparse.Namespace) -> str | None:
    if args.gtpack and Path(args.gtpack).exists():
        return None
    return f"gtpack not found: {args.gtpack}"


def slack_request(method: str, token: str, payload: dict) -> dict:
    body = urllib.parse.urlencode(payload).encode()
    request = urllib.request.Request(
        f"https://slack.com/api/{method}",
        data=body,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/x-www-form-urlencoded",
            "User-Agent": "greentic-provider-e2e",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read().decode())


def run_dummy(info: dict, args: argparse.Namespace) -> dict:
    reason = ensure_gtpack_exists(args)
    if reason:
        return result_base(info, args, "failed", reason)
    result = result_base(info, args, "passed")
    result["provider_message_id"] = f"dummy:{args.correlation_id}"
    return result


def run_slack(info: dict, args: argparse.Namespace) -> dict:
    reason = ensure_gtpack_exists(args)
    if reason:
        return result_base(info, args, "failed", reason)

    token = os.environ["E2E_SLACK_BOT_TOKEN"]
    channel = os.environ["E2E_SLACK_CHANNEL_ID"]
    text = f"Greentic provider e2e {args.correlation_id}"
    response = slack_request(
        "chat.postMessage",
        token,
        {
            "channel": channel,
            "text": text,
            "unfurl_links": "false",
            "unfurl_media": "false",
        },
    )
    if not response.get("ok"):
        return result_base(info, args, "failed", f"Slack API returned error: {response.get('error', 'unknown')}")
    message_ts = response.get("ts")
    if not message_ts:
        return result_base(info, args, "failed", "Slack send succeeded without message ts")

    readback = "not-attempted"
    try:
        history = slack_request(
            "conversations.history",
            token,
            {
                "channel": channel,
                "latest": message_ts,
                "inclusive": "true",
                "limit": "1",
            },
        )
        if history.get("ok") and any(args.correlation_id in item.get("text", "") for item in history.get("messages", [])):
            readback = "verified"
        elif history.get("ok"):
            readback = "message-not-found"
        else:
            readback = f"skipped:{history.get('error', 'unknown')}"
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as err:
        readback = f"skipped:{err.__class__.__name__}"

    result = result_base(info, args, "passed")
    result["provider_message_id"] = message_ts
    result["external_readback"] = readback
    return result


def run_not_implemented(info: dict, args: argparse.Namespace) -> dict:
    return result_base(
        info,
        args,
        "skipped",
        "provider e2e metadata exists, but the live operation is not implemented in this runner yet",
    )


RUNNERS = {
    "dummy": run_dummy,
    "slack": run_slack,
}


def write_summary(result: dict) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return
    with open(summary_path, "a", encoding="utf-8") as handle:
        handle.write("## Provider E2E\n")
        for key in [
            "provider",
            "gtpack_source",
            "version",
            "operation",
            "result",
            "reason",
            "correlation_id",
            "external_readback",
        ]:
            if key in result:
                handle.write(f"- {key}: `{result[key]}`\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    run = subparsers.add_parser("run")
    run.add_argument("--provider", required=True)
    run.add_argument("--gtpack", required=True)
    run.add_argument("--gtpack-source", default="local-build")
    run.add_argument("--correlation-id", default=f"local-{int(time.time())}")
    run.add_argument("--result-json", default="e2e-result.json")

    args = parser.parse_args()
    info = metadata(args.provider)

    for secret in info["e2e"].get("required_secrets", []) + info["e2e"].get("optional_secrets", []):
        value = os.environ.get(secret)
        if value:
            print(f"::add-mask::{value}")

    missing = missing_required_secrets(info)
    if missing:
        result = result_base(info, args, "skipped", "missing required secrets")
        result["missing_secrets"] = missing
    else:
        runner = RUNNERS.get(info["provider"], run_not_implemented)
        result = runner(info, args)

    write_json(Path(args.result_json), result)
    write_summary(result)
    print(json.dumps({k: v for k, v in result.items() if k != "missing_secrets"}, sort_keys=True))
    return 1 if result["result"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
