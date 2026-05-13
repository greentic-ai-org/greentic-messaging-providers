use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::path::PathBuf;

// TODO(force-jump-1.1): re-enable once telegram-webhook is migrated to the
// 0.6.0 invoke contract. The node@0.5.0 export glue (component_entrypoint!)
// was removed from greentic-interfaces-guest >=1.1; the webhook artifact is
// a no-export stub until migration lands.
#[test]
#[ignore = "telegram-webhook on the 1.1 lane is a no-export stub pending 0.6.0 invoke migration"]
fn webhook_telegram_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let values = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/values/webhook_telegram.json");
    let mut cmd = cargo_bin_cmd!("greentic-messaging-tester");
    cmd.arg("webhook")
        .arg("--provider")
        .arg("telegram")
        .arg("--values")
        .arg(values)
        .arg("--public-base-url")
        .arg("https://example.test")
        .arg("--dry-run");
    let output = cmd.assert().success().get_output().stdout.clone();
    let parsed: Value = serde_json::from_slice(&output)?;
    assert_eq!(parsed["expected_url"], "https://example.test");
    assert!(parsed["set_skipped_dry_run"].as_bool().unwrap_or(false));
    Ok(())
}
