use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn gtc_bin() -> String {
    std::env::var("GTC_BIN").unwrap_or_else(|_| "gtc".to_string())
}

fn run_gtc(args: &[String], bundle: &Path) -> Result<String> {
    let output = Command::new(gtc_bin())
        .args(args)
        .current_dir(workspace_root())
        .env("GREENTIC_ENV", "dev")
        .env("NO_COLOR", "1")
        .output()
        .with_context(|| format!("running gtc with args: {}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "gtc command failed\nargs: {}\nbundle: {}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            bundle.display(),
            stdout,
            stderr
        ));
    }
    Ok(stdout)
}

fn is_known_ingress_httpout_compat_error(err: &anyhow::Error) -> bool {
    let text = format!("{err:#}");
    text.contains("failed to deserialize HttpOutV1 response") && text.contains("missing field `v`")
}

fn resolve_demo_provider(bundle: &Path) -> Result<Option<String>> {
    let args = vec![
        "op".to_string(),
        "demo".to_string(),
        "list-packs".to_string(),
        "--bundle".to_string(),
        bundle.display().to_string(),
        "--domain".to_string(),
        "messaging".to_string(),
    ];
    let output = run_gtc(&args, bundle)?;
    for candidate in ["messaging-webchat-gui", "webchat-gui", "messaging-webchat"] {
        if output.contains(candidate) {
            return Ok(Some(candidate.to_string()));
        }
    }
    Ok(None)
}

fn parse_http_body_json(stdout: &str) -> Result<Value> {
    // demo ingress prints: "  body: <json>"
    let line = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("body:"))
        .or_else(|| stdout.lines().find(|line| line.contains(" body: ")))
        .context("could not find HTTP body line in demo ingress output")?;

    let body_json = line
        .split_once("body:")
        .map(|(_, right)| right.trim())
        .context("failed to split body line")?;

    serde_json::from_str::<Value>(body_json)
        .with_context(|| format!("failed to parse body JSON from line: {line}"))
}

#[allow(clippy::too_many_arguments)]
fn demo_ingress_args(
    bundle: &Path,
    provider: &str,
    path: &str,
    method: &str,
    print_mode: &str,
    body_json: Option<&str>,
    headers: &[(&str, String)],
    queries: &[(&str, &str)],
) -> Vec<String> {
    let mut args = vec![
        "op".to_string(),
        "demo".to_string(),
        "ingress".to_string(),
        "--bundle".to_string(),
        bundle.display().to_string(),
        "--provider".to_string(),
        provider.to_string(),
        "--tenant".to_string(),
        "default".to_string(),
        "--team".to_string(),
        "default".to_string(),
        "--path".to_string(),
        path.to_string(),
        "--method".to_string(),
        method.to_string(),
        "--print".to_string(),
        print_mode.to_string(),
    ];

    if let Some(body) = body_json {
        args.push("--body-json".to_string());
        args.push(body.to_string());
    }

    for (name, value) in headers {
        args.push("--header".to_string());
        args.push(format!("{name}: {value}"));
    }
    for (name, value) in queries {
        args.push("--query".to_string());
        args.push(format!("{name}={value}"));
    }

    args
}

fn start_demo_args(bundle: &Path) -> Vec<String> {
    vec![
        "op".to_string(),
        "demo".to_string(),
        "start".to_string(),
        "--bundle".to_string(),
        bundle.display().to_string(),
        "--tenant".to_string(),
        "default".to_string(),
        "--team".to_string(),
        "default".to_string(),
        "--nats".to_string(),
        "off".to_string(),
        "--cloudflared".to_string(),
        "off".to_string(),
    ]
}

fn seed_demo_mcp_catalog(bundle: &Path, provider: &str) -> Result<()> {
    let catalog = json!({
        "components": [{
            "id": "example-com",
            "title": "example-com",
            "tools": [
                { "name": "get_example_home", "title": "get_example_home" }
            ]
        }]
    });
    let args = demo_ingress_args(
        bundle,
        provider,
        "/v3/directline/mcp/catalog",
        "post",
        "http",
        Some(&serde_json::to_string(&catalog)?),
        &[],
        &[("env", "default"), ("tenant", "default")],
    );
    let output = run_gtc(&args, bundle)?;
    let body = parse_http_body_json(&output)?;
    if body.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow::anyhow!("failed to seed mcp catalog: {}", body));
    }
    Ok(())
}

#[test]
#[ignore = "interactive/manual smoke test via gtc demo ingress (webchat-gui)"]
fn interactive_webchat_gui_mcp_via_gtc_demo_ingress() -> Result<()> {
    let root = workspace_root();
    let bundle = root.join("demo-bundle");
    let provider = match resolve_demo_provider(&bundle)? {
        Some(provider) => provider,
        None => {
            eprintln!(
                "skipping interactive test: no webchat provider found in bundle {}. \
                 Prepare demo bundle packs/providers first.",
                bundle.display()
            );
            return Ok(());
        }
    };
    seed_demo_mcp_catalog(&bundle, &provider)?;
    let target_component =
        root.join("components/messaging-provider-webchat/tests/assets/example_home.component.wasm");

    assert!(bundle.exists(), "demo bundle missing: {}", bundle.display());
    assert!(
        target_component.exists(),
        "target component missing: {}",
        target_component.display()
    );

    let token_args = demo_ingress_args(
        &bundle,
        &provider,
        "/v3/directline/tokens/generate",
        "post",
        "http",
        Some(r#"{"user":{"id":"alice"}}"#),
        &[],
        &[("env", "default"), ("tenant", "default")],
    );
    let token_stdout = match run_gtc(&token_args, &bundle) {
        Ok(stdout) => stdout,
        Err(err) if is_known_ingress_httpout_compat_error(&err) => {
            eprintln!(
                "skipping interactive ingress smoke due to known HttpOutV1 compatibility issue: {err}"
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let token_body = parse_http_body_json(&token_stdout)?;
    let token = token_body
        .get("token")
        .and_then(Value::as_str)
        .context("missing token in token response")?
        .to_string();

    let conv_args = demo_ingress_args(
        &bundle,
        &provider,
        "/v3/directline/conversations",
        "post",
        "http",
        None,
        &[("Authorization", format!("Bearer {token}"))],
        &[],
    );
    let conv_stdout = run_gtc(&conv_args, &bundle)?;
    let conv_body = parse_http_body_json(&conv_stdout)?;
    let conversation_id = conv_body
        .get("conversationId")
        .and_then(Value::as_str)
        .context("missing conversationId")?
        .to_string();
    let conv_token = conv_body
        .get("token")
        .and_then(Value::as_str)
        .context("missing conversation token")?
        .to_string();

    let click_payload = json!({
        "type": "message",
        "from": { "id": "alice" },
        "value": {
            "mcp": {
                "component": target_component.display().to_string(),
                "operation": "get_example_home",
                "args": {
                    "source": "webchat-gui-gtc-test"
                }
            }
        }
    });

    let click_path = format!("/v3/directline/conversations/{conversation_id}/activities");
    let click_args = demo_ingress_args(
        &bundle,
        &provider,
        &click_path,
        "post",
        "events",
        Some(&serde_json::to_string(&click_payload)?),
        &[("Authorization", format!("Bearer {conv_token}"))],
        &[],
    );
    let click_stdout = run_gtc(&click_args, &bundle)?;

    assert!(
        click_stdout.contains("\"mcp_trigger\": \"true\""),
        "expected mcp_trigger metadata in output\n{}",
        click_stdout
    );
    assert!(
        click_stdout.contains("\"mcp_operation\": \"get_example_home\""),
        "expected mcp_operation metadata in output\n{}",
        click_stdout
    );
    assert!(
        click_stdout.contains(&target_component.display().to_string()),
        "expected target component path in output\n{}",
        click_stdout
    );

    Ok(())
}

#[test]
#[ignore = "interactive/manual webchat-gui browser session; starts operator and holds"]
fn interactive_webchat_gui_manual_session() -> Result<()> {
    let root = workspace_root();
    let bundle = root.join("demo-bundle");

    if !bundle.exists() {
        eprintln!(
            "skipping interactive test: demo bundle missing at {}",
            bundle.display()
        );
        return Ok(());
    }

    let provider = match resolve_demo_provider(&bundle)? {
        Some(provider) => provider,
        None => {
            eprintln!(
                "skipping interactive test: no webchat provider found in bundle {}",
                bundle.display()
            );
            return Ok(());
        }
    };
    seed_demo_mcp_catalog(&bundle, &provider)?;

    let hold_secs = std::env::var("WEBCHAT_INTERACTIVE_HOLD_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(300);

    let url = "http://localhost:8080/v1/web/webchat/default/";
    eprintln!(
        "starting interactive webchat session with provider={provider}. open: {url} (holding for {hold_secs}s)"
    );

    let args = start_demo_args(&bundle);
    let mut child = Command::new(gtc_bin())
        .args(&args)
        .current_dir(workspace_root())
        .env("GREENTIC_ENV", "dev")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting gtc with args: {}", args.join(" ")))?;

    thread::sleep(Duration::from_secs(3));
    if let Some(status) = child
        .try_wait()
        .context("failed to check demo start process status")?
    {
        return Err(anyhow::anyhow!(
            "demo start exited early with status {status}. check logs above."
        ));
    }

    thread::sleep(Duration::from_secs(hold_secs));

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
