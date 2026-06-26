#!/usr/bin/env python3
"""
Drive the Microsoft Teams (Bot Framework) setup wizard end-to-end via the
backend setup API, bypassing the wizard UI — whose device-code polling can stall
after the code is authorized (it keeps "polling" even though the token was
already obtained).

This tool polls the backend STATE (the authoritative signal), re-issues expired
device codes automatically, and walks all 7 steps. Run it in your own terminal
(it prompts you to authorize each device code).

Usage:
  scripts/teams_setup_walkthrough.py \
      --public-url https://<tunnel>.trycloudflare.com \
      [--base http://127.0.0.1:8080/v1/messaging/setup/messaging-teams/demo]
"""
import argparse
import json
import time
import urllib.error
import urllib.request

DEFAULT_BASE = "http://127.0.0.1:8080/v1/messaging/setup/messaging-teams/demo"

# step_id -> (issue endpoint, poll endpoint, who to sign in as)
DEVICE_STEPS = {
    "graph_admin_consent": (
        "oauth/start",
        "oauth/complete",
        "your Microsoft 365 ADMIN account",
    ),
    "microsoft_bot_channel_registration_consent": (
        "oauth/management/start",
        "oauth/management/complete",
        "the Azure account that OWNS the subscription",
    ),
}


def call(base, path, method="GET", obj=None, timeout=120):
    data = json.dumps(obj).encode() if obj is not None else None
    req = urllib.request.Request(
        base + path, data=data, headers={"content-type": "application/json"}, method=method
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        return {"_http_error": e.code, "_body": e.read().decode("utf-8", "replace")[:300]}


def find(o, key):
    if isinstance(o, dict):
        for k, v in o.items():
            if k == key and v not in (None, ""):
                return v
            r = find(v, key)
            if r is not None:
                return r
    elif isinstance(o, list):
        for it in o:
            r = find(it, key)
            if r is not None:
                return r
    return None


def items(state):
    return (state.get("setup_status") or {}).get("items") or []


def step_state(state, step_id):
    for s in items(state):
        if s.get("id") == step_id:
            return s.get("state")
    return None


def progress(state):
    its = items(state)
    return sum(1 for s in its if s.get("state") == "done"), len(its)


def pending_step(state):
    for s in items(state):
        if s.get("state") != "done":
            return s.get("id")
    return None


def error_text(o):
    acc = []

    def walk(x):
        if isinstance(x, dict):
            for k, v in x.items():
                if k in ("error", "error_description") and isinstance(v, str) and v.strip():
                    acc.append(v.strip())
                walk(v)
        elif isinstance(x, list):
            for it in x:
                walk(it)

    walk(o)
    return " | ".join(acc)


def run_device_step(base, step_id):
    start, poll, who = DEVICE_STEPS[step_id]
    while True:  # re-issue loop
        issued = call(base, "/" + start, "POST", {})
        code = find(issued, "user_code")
        url = find(issued, "oauth_verification_uri") or "https://microsoft.com/devicelogin"
        print(f"\n  ┌─ {step_id}")
        print(f"  │  1) open  {url}")
        print(f"  │  2) enter {code}")
        print(f"  │  3) sign in as {who}")
        input("  └─ press ENTER after you have approved… ")
        # poll the backend STATE until the step flips to done (ignore stale UI text)
        for _ in range(40):
            resp = call(base, "/" + poll, "POST", {})
            if step_state(resp, step_id) == "done":
                print(f"  ✓ {step_id} complete")
                return
            errs = error_text(resp).lower()
            if "aadsts70008" in errs or "expired" in errs or "declined" in errs:
                print("  ⟳ code expired/declined — issuing a fresh one")
                break
            time.sleep(3)  # authorization_pending — keep polling
        else:
            print("  ⟳ timed out — issuing a fresh code")


def ensure_mgmt_token_in_config(base):
    """Safety net for older packs: copy the ARM token from values into config."""
    st = call(base, "/")
    tok = (st.get("values") or {}).get("azure_management_access_token")
    if tok:
        call(base, "/config", "POST", {"config": {"azure_management_access_token": tok}})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--public-url", required=True, help="public base URL (cloudflare tunnel)")
    ap.add_argument("--base", default=DEFAULT_BASE, help="setup API base for the tenant")
    args = ap.parse_args()
    base = args.base.rstrip("/")

    call(base, "/config", "POST", {"config": {"public_base_url": args.public_url}})
    print(f"public_base_url = {args.public_url}")

    while True:
        state = call(base, "/")
        done, total = progress(state)
        step = pending_step(state)
        print(f"\n[{done}/{total}] next → {step}")

        if step is None:
            print("\n🎉 all steps complete — message the bot in Teams to finish if not already.")
            return

        if step in DEVICE_STEPS:
            run_device_step(base, step)
            continue

        if step == "first_bot_framework_post":
            print("  → send a message to the bot in Microsoft Teams now…")
            for _ in range(60):
                if step_state(call(base, "/"), step) == "done":
                    print("  ✓ first bot message received — setup complete")
                    return
                time.sleep(5)
            print("  (still waiting — re-run once you've messaged the bot)")
            return

        # advance step (bot_app_identity, bot_framework_endpoint_registration, publish, install)
        if step == "bot_framework_endpoint_registration":
            ensure_mgmt_token_in_config(base)
        resp = call(base, "/next", "POST", {})
        if step_state(resp, step) == "done":
            print(f"  ✓ {step} complete")
            continue
        # did not advance — surface the blocker and stop
        hint = find(resp, "next")
        print(f"  ✗ {step} did not advance")
        if error_text(resp):
            print(f"    error: {error_text(resp)}")
        if hint:
            print(f"    hint:  {hint}")
        return


if __name__ == "__main__":
    main()
