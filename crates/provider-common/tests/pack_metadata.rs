use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow};
use serde_json::{Map, Value};
use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn build_gtpack(src_dir: &Path, output: &Path) -> Result<()> {
    let file = fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(src_dir).expect("relative path");
            let mut contents = Vec::new();
            fs::File::open(&path)?.read_to_end(&mut contents)?;
            zip.start_file(rel.to_string_lossy(), options)?;
            zip.write_all(&contents)?;
        }
    }

    zip.finish()?;
    Ok(())
}

fn read_from_gtpack(gtpack: &Path, file: &str) -> Result<Vec<u8>> {
    let archive = fs::File::open(gtpack)?;
    let mut zip = zip::ZipArchive::new(archive)?;
    let mut file = zip.by_name(file)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn run_metadata_generator(workspace_root: &Path, pack_dir: &Path) {
    let status = Command::new("python3")
        .arg(workspace_root.join("tools/generate_pack_metadata.py"))
        .arg("--pack-dir")
        .arg(pack_dir)
        .arg("--components-dir")
        .arg(workspace_root.join("components"))
        .arg("--include-capabilities-cache")
        .arg("--version")
        .arg("test")
        .status()
        .expect("failed to run metadata generator");
    assert!(status.success(), "metadata generator did not exit cleanly");
}

fn manifest_components(manifest_path: &Path) -> Result<Vec<String>> {
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    let comps = manifest
        .get("components")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("manifest missing components array"))?;
    Ok(comps
        .iter()
        .filter_map(Value::as_str)
        .map(|s| s.to_string())
        .collect())
}

fn collect_expected_requirements(
    components_dir: &Path,
    component_names: &[String],
) -> Result<BTreeMap<(String, String), Map<String, Value>>> {
    let mut merged: BTreeMap<(String, String), Map<String, Value>> = BTreeMap::new();
    for component in component_names {
        let manifest_path = components_dir
            .join(component)
            .join("component.manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let manifest: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
        if let Some(reqs) = manifest
            .get("secret_requirements")
            .and_then(|v| v.as_array())
        {
            for req in reqs {
                let obj = req
                    .as_object()
                    .cloned()
                    .ok_or_else(|| anyhow!("requirement must be an object"))?;
                let name = obj
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("requirement missing name"))?
                    .to_string();
                let scope = obj
                    .get("scope")
                    .and_then(Value::as_str)
                    .unwrap_or("tenant")
                    .to_string();
                merged.entry((name, scope)).or_insert(obj);
            }
        }
    }
    Ok(merged)
}

fn requirement_keys(
    requirements: &[Value],
) -> Result<BTreeMap<(String, String), Map<String, Value>>> {
    let mut merged: BTreeMap<(String, String), Map<String, Value>> = BTreeMap::new();
    for req in requirements {
        let obj = req
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow!("requirement must be an object"))?;
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("requirement missing name"))?
            .to_string();
        let scope = obj
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("tenant")
            .to_string();
        merged.insert((name, scope), obj);
    }
    Ok(merged)
}

#[test]
fn gtpack_contains_secret_requirements_metadata() -> Result<()> {
    let root = workspace_root();
    let pack_source = root.join("packs").join("messaging-telegram");
    let temp = tempdir()?;
    let pack_copy = temp.path().join("messaging-telegram");
    copy_dir(&pack_source, &pack_copy)?;

    run_metadata_generator(&root, &pack_copy);

    let gtpack_path = temp.path().join("messaging-telegram.gtpack");
    build_gtpack(&pack_copy, &gtpack_path)?;

    let manifest_bytes = read_from_gtpack(&gtpack_path, "pack.manifest.json")?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;

    let schema_path = manifest
        .get("config_schema")
        .and_then(|v| v.get("provider_config"))
        .and_then(|v| v.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("pack manifest missing config schema path"))?;
    assert_eq!(
        schema_path, "schemas/messaging/telegram/public.config.schema.json",
        "unexpected config schema path for messaging-telegram"
    );

    let requirements = manifest
        .get("secret_requirements")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("pack manifest missing secret_requirements"))?;

    assert!(
        !requirements.is_empty(),
        "secret_requirements should be populated for messaging-telegram"
    );

    let components = manifest_components(&pack_copy.join("pack.manifest.json"))?;
    let expected = collect_expected_requirements(&root.join("components"), &components)?;
    let actual = requirement_keys(requirements)?;

    assert_eq!(
        requirements.len(),
        actual.len(),
        "secret_requirements should be deduplicated by name+scope"
    );

    let expected_keys: BTreeSet<_> = expected.keys().cloned().collect();
    let actual_keys: BTreeSet<_> = actual.keys().cloned().collect();
    assert_eq!(
        expected_keys, actual_keys,
        "secret requirement keys should match component manifests"
    );

    for key in expected_keys {
        let expected_req = expected.get(&key).unwrap();
        let actual_req = actual.get(&key).unwrap();
        assert_eq!(
            actual_req.get("description"),
            expected_req.get("description"),
            "description should be preserved for {:?}",
            key
        );
        assert_eq!(
            actual_req.get("example"),
            expected_req.get("example"),
            "example should be preserved for {:?}",
            key
        );
        for field in actual_req.keys() {
            assert!(
                matches!(
                    field.as_str(),
                    "name"
                        | "scope"
                        | "description"
                        | "example"
                        | "required"
                        | "aliases"
                        | "generated"
                ),
                "unexpected field {} in requirement {:?}",
                field,
                key
            );
        }
    }

    let cache = manifest
        .get("capabilities_cache")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("pack manifest missing capabilities_cache"))?;
    for entry in cache {
        let obj = entry
            .as_object()
            .ok_or_else(|| anyhow!("capabilities_cache entry must be object"))?;
        let component = obj
            .get("component")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("capabilities_cache entry missing component"))?;
        assert!(
            components.contains(&component.to_string()),
            "capabilities_cache component {} not in manifest components",
            component
        );
        let path = obj
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("capabilities_cache entry missing path"))?;
        let cache_bytes = read_from_gtpack(&gtpack_path, path)?;
        assert!(
            !cache_bytes.is_empty(),
            "capabilities cache file {} should be present",
            path
        );
    }

    Ok(())
}

#[test]
fn generated_runtime_secret_metadata_is_declared_for_seedable_provider_secrets() -> Result<()> {
    let root = workspace_root();
    let expected = [
        (
            "messaging-webex",
            "webex_webhook_secret",
            "WEBEX_WEBHOOK_SECRET",
        ),
        (
            "messaging-webchat-gui",
            "jwt_signing_key",
            "JWT_SIGNING_KEY",
        ),
        ("messaging-webchat", "jwt_signing_key", "JWT_SIGNING_KEY"),
    ];

    for (pack, secret_name, alias) in expected {
        let manifest_path = root.join("packs").join(pack).join("pack.manifest.json");
        let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        let extension_secret = manifest
            .get("extensions")
            .and_then(|value| value.get("greentic.generated-secrets.v1"))
            .and_then(|value| value.get("inline"))
            .and_then(|value| value.get("secrets"))
            .and_then(Value::as_array)
            .and_then(|secrets| {
                secrets
                    .iter()
                    .find(|value| value.get("key").and_then(Value::as_str) == Some(secret_name))
            })
            .ok_or_else(|| {
                anyhow!("{pack} missing greentic.generated-secrets.v1 entry for {secret_name}")
            })?;

        let requirements = manifest
            .get("secret_requirements")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{pack} missing secret_requirements"))?;
        assert!(
            !requirements
                .iter()
                .any(|value| value.get("name").and_then(Value::as_str) == Some(secret_name)),
            "{pack} generated secret {secret_name} must not be in secret_requirements; setup treats those as user-provided"
        );
        assert!(
            extension_secret
                .get("aliases")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|value| value.as_str() == Some(alias)),
            "{pack} generated-secret extension must declare env alias {alias}"
        );
        assert_eq!(
            extension_secret.get("policy").and_then(Value::as_str),
            Some("random")
        );
        assert_eq!(
            extension_secret.get("length").and_then(Value::as_u64),
            Some(20)
        );
        assert_eq!(
            extension_secret.get("encoding").and_then(Value::as_str),
            Some("raw_text")
        );
        assert_eq!(
            extension_secret
                .get("scope")
                .and_then(|value| value.get("level"))
                .and_then(Value::as_str),
            Some("tenant")
        );
        assert_eq!(
            extension_secret
                .get("scope")
                .and_then(|value| value.get("team"))
                .and_then(Value::as_str),
            Some("_")
        );
        assert_eq!(
            extension_secret
                .get("regenerate_if_present")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    Ok(())
}

#[test]
fn slack_signing_secret_is_registered_not_generated() -> Result<()> {
    let root = workspace_root();
    let manifest_path = root
        .join("packs")
        .join("messaging-slack")
        .join("pack.manifest.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let generated = manifest
        .get("extensions")
        .and_then(|value| value.get("greentic.generated-secrets.v1"))
        .and_then(|value| value.get("inline"))
        .and_then(|value| value.get("secrets"))
        .and_then(Value::as_array);

    assert!(
        generated.is_none_or(|secrets| secrets.iter().all(|value| {
            value.get("key").and_then(Value::as_str) != Some("slack_signing_secret")
        })),
        "Slack signing secret comes from Slack app registration and must not be generated by start"
    );

    let requirements = manifest
        .get("secret_requirements")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("messaging-slack missing secret_requirements"))?;
    assert!(
        !requirements.iter().any(|value| {
            value.get("name").and_then(Value::as_str) == Some("slack_signing_secret")
                || value.get("name").and_then(Value::as_str) == Some("SLACK_SIGNING_SECRET")
        }),
        "Slack signing secret must not be setup-prompted; setup_app_registration stores Slack's returned value"
    );
    let secrets_out = manifest
        .get("extensions")
        .and_then(|value| value.get("greentic.provider-extension.v1"))
        .and_then(|value| value.get("inline"))
        .and_then(|value| value.get("providers"))
        .and_then(Value::as_array)
        .and_then(|providers| providers.first())
        .and_then(|provider| provider.get("setup_contract"))
        .and_then(|contract| contract.get("secrets_out"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("messaging-slack missing provider setup_contract.secrets_out"))?;
    assert!(
        secrets_out.iter().any(|value| {
            value.get("answer_key").and_then(Value::as_str) == Some("slack_signing_secret")
                && value.get("secret_key").and_then(Value::as_str) == Some("SLACK_SIGNING_SECRET")
        }),
        "Slack setup contract must persist Slack's returned signing secret as SLACK_SIGNING_SECRET"
    );

    Ok(())
}

// packs_lock_has_digest only pins messaging-telegram, so a rebuilt pack committed
// without rerunning tools/update_packs_lock.py went unnoticed for every other pack.
#[test]
fn packs_lock_digests_match_committed_artifacts() -> Result<()> {
    use sha2::{Digest, Sha256};

    let root = workspace_root();
    let lock_path = root.join("packs.lock.json");
    if !lock_path.exists() {
        eprintln!("Skipping: {} not found", lock_path.display());
        return Ok(());
    }

    let lock_json: Value = serde_json::from_slice(&std::fs::read(&lock_path)?)?;
    let packs = lock_json
        .get("packs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("packs.lock.json missing packs array"))?;

    let mut stale = Vec::new();
    for entry in packs {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("packs.lock.json entry missing name"))?;
        let file = entry
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{name}: packs.lock.json entry missing file"))?;
        let artifact = root.join(file);
        // A pack published from a workflow leaves no artifact in the tree to compare.
        if !artifact.exists() {
            continue;
        }
        let digest = entry
            .get("digest")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{name}: packs.lock.json entry missing digest"))?;
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(&artifact)?);
        let actual = format!("sha256:{}", to_hex(hasher.finalize()));
        if actual != digest {
            stale.push(format!(
                "{name}: lock says {digest}, {file} hashes to {actual}"
            ));
        }
    }

    if !stale.is_empty() {
        return Err(anyhow!(
            "packs.lock.json is stale for {} pack(s) — rerun `python3 tools/update_packs_lock.py`:\n  {}",
            stale.len(),
            stale.join("\n  ")
        ));
    }
    Ok(())
}

#[test]
fn packs_lock_has_digest() -> Result<()> {
    use sha2::{Digest, Sha256};

    let root = workspace_root();
    let lock_path = root.join("packs.lock.json");
    let gtpack_path = root
        .join("dist")
        .join("packs")
        .join("messaging-telegram.gtpack");

    // Skip test if required build artifacts don't exist yet (e.g., early CI stage)
    if !lock_path.exists() {
        eprintln!(
            "Skipping packs_lock_has_digest: {} not found (run update_packs_lock.py after building packs)",
            lock_path.display()
        );
        return Ok(());
    }
    if !gtpack_path.exists() {
        eprintln!(
            "Skipping packs_lock_has_digest: {} not found (build packs first)",
            gtpack_path.display()
        );
        return Ok(());
    }

    let lock_json: Value = serde_json::from_slice(&std::fs::read(&lock_path)?)?;
    let packs = lock_json
        .get("packs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("packs.lock.json missing packs array"))?;
    let entry = packs
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some("messaging-telegram"))
        .ok_or_else(|| anyhow!("packs.lock.json missing messaging-telegram entry"))?;
    assert!(
        !packs
            .iter()
            .any(|p| p.get("name").and_then(Value::as_str) == Some("messaging-provider-bundle")),
        "packs.lock.json should not include messaging-provider-bundle"
    );
    let bundle_path = root
        .join("dist")
        .join("packs")
        .join("messaging-provider-bundle.gtpack");
    assert!(
        !bundle_path.exists(),
        "bundle pack artifact should not exist at {}",
        bundle_path.display()
    );
    let digest = entry
        .get("digest")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("packs.lock.json missing digest"))?;
    let bytes = std::fs::read(&gtpack_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hex = to_hex(hasher.finalize());
    assert_eq!(digest, format!("sha256:{hex}"));
    Ok(())
}

fn to_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn webchat_gui_pack_declares_provider_routes_and_static_assets() -> Result<()> {
    let root = workspace_root();
    let pack_dir = root.join("packs").join("messaging-webchat-gui");
    let manifest_path = pack_dir.join("pack.manifest.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;

    let provider = manifest
        .get("extensions")
        .and_then(|ext| ext.get("greentic.provider-extension.v1"))
        .and_then(|ext| ext.get("inline"))
        .and_then(|inline| inline.get("providers"))
        .and_then(Value::as_array)
        .and_then(|providers| providers.first())
        .ok_or_else(|| anyhow!("webchat-gui manifest missing provider extension entry"))?;
    assert_eq!(
        provider.get("provider_type").and_then(Value::as_str),
        Some("messaging.webchat-gui")
    );
    assert_eq!(
        provider
            .get("runtime")
            .and_then(|rt| rt.get("component_ref"))
            .and_then(Value::as_str),
        Some("messaging-provider-webchat-gui")
    );
    assert_eq!(
        provider.get("config_schema_ref").and_then(Value::as_str),
        Some("schemas/messaging/webchat-gui/public.config.schema.json")
    );

    let http_routes = manifest
        .get("extensions")
        .and_then(|ext| ext.get("greentic.http-routes.v1"))
        .and_then(|ext| ext.get("inline"))
        .ok_or_else(|| anyhow!("webchat-gui manifest missing http-routes.v1"))?;
    let routes = http_routes
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("webchat-gui manifest missing http-routes array"))?;
    assert!(
        routes.iter().any(|route| {
            route
                .get("pattern")
                .and_then(Value::as_str)
                .is_some_and(|p| p.contains("/token"))
        }),
        "expected token route in http-routes"
    );
    assert!(
        routes.iter().any(|route| {
            route
                .get("pattern")
                .and_then(Value::as_str)
                .is_some_and(|p| p.contains("/v3/directline/"))
        }),
        "expected directline route in http-routes"
    );

    let static_route = manifest
        .get("extensions")
        .and_then(|ext| ext.get("greentic.static-routes.v1"))
        .and_then(|ext| ext.get("inline"))
        .and_then(|inline| inline.get("routes"))
        .and_then(Value::as_array)
        .and_then(|routes| routes.first())
        .ok_or_else(|| anyhow!("webchat-gui manifest missing static route"))?;
    assert_eq!(
        static_route.get("public_path").and_then(Value::as_str),
        Some("/v1/web/webchat/{tenant}")
    );
    assert_eq!(
        static_route.get("source_root").and_then(Value::as_str),
        Some("assets/webchat-gui")
    );
    assert_eq!(
        static_route
            .get("scope")
            .and_then(|scope| scope.get("tenant"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        static_route
            .get("scope")
            .and_then(|scope| scope.get("team"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        static_route.get("index_file").and_then(Value::as_str),
        Some("index.html")
    );
    assert_eq!(
        static_route.get("spa_fallback").and_then(Value::as_str),
        Some("index.html")
    );

    let assets = manifest
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("webchat-gui manifest missing assets"))?;
    assert!(
        assets.iter().any(|asset| {
            asset
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| path == "assets/webchat-gui/embed.js")
        }),
        "pack.manifest.json should include embed.js"
    );

    let staged_component = pack_dir.join("components/messaging-provider-webchat-gui.wasm");
    assert!(
        staged_component.exists(),
        "expected staged component artifact at {}",
        staged_component.display()
    );

    Ok(())
}

#[test]
fn webchat_gui_packaged_component_exports_ingress_world() -> Result<()> {
    let root = workspace_root();
    let gtpack_path = root
        .join("dist")
        .join("packs")
        .join("messaging-webchat-gui.gtpack");
    let component = read_from_gtpack(
        &gtpack_path,
        "components/messaging-provider-webchat-gui.wasm",
    )?;
    let tmp = tempdir()?;
    let wasm_path = tmp.path().join("messaging-provider-webchat-gui.wasm");
    fs::write(&wasm_path, component)?;

    let output = Command::new("wasm-tools")
        .arg("component")
        .arg("wit")
        .arg(&wasm_path)
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "wasm-tools component wit failed for {}: {}",
            wasm_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let wit = String::from_utf8(output.stdout)?;
    assert!(
        wit.contains("export provider:common/ingress@0.0.2;"),
        "packaged messaging-provider-webchat-gui.wasm must export provider:common/ingress@0.0.2 for webchat HTTP routes"
    );
    assert!(
        wit.contains("handle-webhook: func("),
        "packaged messaging-provider-webchat-gui.wasm ingress world must expose handle-webhook"
    );

    Ok(())
}

#[test]
fn messaging_teams_source_is_answer_owned_with_setup_assets() -> Result<()> {
    let root = workspace_root();
    let source_dir = root.join("messaging-teams");
    let answer_path = source_dir.join("build-answer.json");
    let answer: Value = serde_json::from_slice(&fs::read(&answer_path)?)?;
    let pack_version = answer
        .get("pack_version")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("messaging-teams build-answer missing pack_version"))?;

    assert_eq!(
        answer.get("schema_id").and_then(Value::as_str),
        Some("greentic-pack.wizard.answers")
    );
    assert_eq!(
        answer
            .get("build_answer")
            .and_then(|meta| meta.get("schema_id"))
            .and_then(Value::as_str),
        Some("greentic-messaging-teams.build-answer")
    );
    assert!(
        answer.get("pack_create").is_some(),
        "messaging-teams must declare pack_create answers"
    );
    assert!(
        answer.get("pack").is_some(),
        "messaging-teams must declare pack update answers"
    );
    assert_eq!(
        answer
            .get("source_layout")
            .and_then(|layout| layout.get("assets"))
            .and_then(Value::as_str),
        Some("assets")
    );
    assert_eq!(
        answer
            .get("source_layout")
            .and_then(|layout| layout.get("src"))
            .and_then(Value::as_str),
        Some("src")
    );

    let generated_pack_dir = root.join("packs").join("messaging-teams");
    assert!(
        !generated_pack_dir.exists(),
        "packs/messaging-teams is generated output; source must live under messaging-teams/"
    );

    let assets = answer
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("messaging-teams build-answer missing assets"))?;
    for expected in [
        "assets/setup/greentic-teams-setup.js",
        "assets/setup/greentic-teams-setup.d.ts",
        "assets/setup/README.md",
        "assets/setup/examples/basic.html",
        "assets/setup/conformance.json",
        "assets/setup/backend-contract.json",
        "assets/setup.yaml",
        "assets/teams-app/manifest.template.json",
    ] {
        assert!(
            assets.iter().any(|asset| asset.as_str() == Some(expected)),
            "messaging-teams build-answer missing asset {expected}"
        );
        assert!(
            source_dir.join(expected).exists(),
            "messaging-teams asset missing on disk: {expected}"
        );
    }

    let setup_api = answer
        .get("setup_api")
        .ok_or_else(|| anyhow!("messaging-teams build-answer missing setup_api"))?;
    let web_component = setup_api
        .get("web_component")
        .ok_or_else(|| anyhow!("messaging-teams build-answer missing setup_api.web_component"))?;
    assert_eq!(
        web_component.get("schema_id").and_then(Value::as_str),
        Some("greentic.setup.web-component.v1")
    );
    assert_eq!(
        web_component.get("tag_name").and_then(Value::as_str),
        Some("greentic-teams-setup-v4")
    );
    assert_eq!(
        web_component.get("module_asset").and_then(Value::as_str),
        Some("assets/setup/greentic-teams-setup.js")
    );
    let expected_module_url = format!(
        "/v1/web/messaging-teams/setup/{{tenant}}/greentic-teams-setup.js?v={pack_version}-setup4"
    );
    assert_eq!(
        web_component.get("module_url").and_then(Value::as_str),
        Some(expected_module_url.as_str())
    );
    assert_eq!(
        web_component.get("asset_base_path").and_then(Value::as_str),
        Some("/v1/web/messaging-teams/setup/{tenant}")
    );
    assert_eq!(
        web_component
            .get("completion")
            .and_then(|value| value.get("event"))
            .and_then(Value::as_str),
        Some("greentic-provider-setup-complete")
    );
    assert_eq!(
        web_component
            .get("completion")
            .and_then(|value| value.get("state_path"))
            .and_then(Value::as_str),
        Some("setup_status.ok")
    );
    assert_eq!(
        web_component
            .get("attributes")
            .and_then(|value| value.get("state-path"))
            .and_then(Value::as_str),
        Some("/v1/messaging/setup/messaging-teams/{tenant}")
    );
    let backend_contract = setup_api.get("backend_contract").ok_or_else(|| {
        anyhow!("messaging-teams build-answer missing setup_api.backend_contract")
    })?;
    assert_eq!(
        backend_contract.get("schema_id").and_then(Value::as_str),
        Some("greentic.setup.backend-contract.v1")
    );
    assert_eq!(
        backend_contract.get("asset").and_then(Value::as_str),
        Some("assets/setup/backend-contract.json")
    );

    let setup_api = setup_api
        .get("routes")
        .ok_or_else(|| anyhow!("messaging-teams build-answer missing setup_api.routes"))?;
    assert_eq!(
        setup_api.get("state").and_then(Value::as_str),
        Some("/v1/messaging/setup/messaging-teams/{tenant}")
    );
    assert_eq!(
        setup_api.get("oauth_complete").and_then(Value::as_str),
        Some("/v1/messaging/setup/messaging-teams/{tenant}/oauth/{kind}/complete")
    );

    let component = fs::read_to_string(source_dir.join("assets/setup/greentic-teams-setup.js"))?;
    let teams_manifest =
        fs::read_to_string(source_dir.join("assets/teams-app/manifest.template.json"))?;
    assert!(
        teams_manifest.contains("\"short\": \"Greentic Teams Bot\"")
            && teams_manifest.contains("\"botId\": \"{bot_app_id}\"")
            && teams_manifest.contains("\"id\": \"{teams_app_id}\"")
            && !teams_manifest.contains("{app_name}")
            && !teams_manifest.contains("{public_domain}"),
        "Teams app manifest must only use setup-supported placeholders"
    );
    assert!(
        component.contains("state-path") && component.contains("oauth-complete-path"),
        "setup component must expose endpoint path attributes"
    );
    assert!(
        component.contains("provider-id")
            && component.contains("greentic-provider-setup-")
            && component.contains("_emitCompleteIfDone"),
        "setup component must expose generic provider setup web-component events"
    );
    assert!(
        component.contains("\"greentic-teams-setup-v4\""),
        "setup component must register a versioned custom element tag so browser sessions cannot reuse an old class"
    );
    assert!(
        component.contains("class extends GreenticTeamsSetup"),
        "setup component must use a fresh constructor for the versioned custom element tag"
    );
    assert!(
        !component.contains("data-role=\"requiredFields\""),
        "setup component must not inject provider-specific Microsoft credential fields into the main action"
    );
    assert!(
        component.contains("_draftConfig") && component.contains("result && result.ok === false"),
        "setup component must preserve edited advanced config and stop polling on validation failures"
    );
    assert!(
        component.contains("open-device-login")
            && component.contains("_deviceLoginPollMs")
            && component.contains("_actionDeadline")
            && component.contains("authorizationPending"),
        "setup component must keep device login controls visible and respect Microsoft device-flow polling"
    );
    assert!(
        component.contains("let waitAction = action")
            && component.contains("waitAction = {")
            && component.contains("kind: \"device-login\""),
        "setup component must switch a continue action into device-login waiting once Microsoft returns a device code"
    );
    assert!(
        component.contains("_activeDeviceLogin")
            && component.contains("_oauthComplete")
            && component.contains("values.oauth"),
        "setup component must ignore stale device-login codes once server-side OAuth state is complete"
    );
    assert!(
        component.contains("verifyTeamsInstall")
            && component.contains("addToTeamsOpened")
            && component.contains("verifyBotMessage")
            && component.contains("openBotChatOpened")
            && component.contains("_waitingForFirstBotMessage")
            && component.contains("_writeManualActions"),
        "setup component must verify Teams installation and the first bot message after opening manual Teams browser actions"
    );
    assert!(
        component.contains("_clientConfig")
            && component.contains("\"oauth_device_code\"")
            && component.contains("\"graph_access_token\"")
            && component.contains("\"azure_management_access_token\""),
        "setup component must strip server-owned OAuth/token state, including legacy management tokens, before posting browser config"
    );
    assert!(
        component.contains("\"azure_management_user_code\"")
            && component.contains("oauthKind === \"management\" ? \"azure_management_user_code\" : \"oauth_user_code\""),
        "setup component must keep Graph and Azure management device-login user codes separate"
    );
    assert!(
        component.contains("\"public_base_url\"")
            && !component.contains("  \"bot_framework_registration_url\",\n"),
        "setup component advanced config must expose public URL but not ask admins for setup-host registration internals"
    );
    assert!(
        component.contains("_preflightAction")
            && component.contains("_localError(preflight)")
            && component.contains("bot_framework_endpoint_registration")
            && component.contains("missingPublicBaseUrl")
            && !component.contains("bot_framework_registration_url"),
        "setup component must require public runtime URL without exposing setup-host registration internals"
    );
    assert!(
        component.contains("result && result.setup_status")
            && component.contains("this._applyState(result)")
            && component.contains("nextResult && nextResult.setup_status")
            && component.contains("this._applyState(nextResult)")
            && component.contains("this._advanced(before, after, waitAction)")
            && component.contains("const nextResult = await this._request(\"POST\", this._endpoint(\"next\"), this._collectConfig())"),
        "setup component must apply immediate OAuth completion and /next state responses before polling for advancement"
    );
    assert!(
        component.contains("request-start")
            && component.contains("request-success")
            && component.contains("request-error")
            && component.contains("skip-next")
            && component.contains("pendingStepId"),
        "setup component must emit request and skip diagnostics for setup backend calls"
    );
    assert!(
        component.contains("_isStaleState(nextState)")
            && component.contains("next.done < current.done")
            && component.contains("after.done === before.done && action.kind === \"device-login\""),
        "setup component must not let stale setup-state refreshes move the wizard backward or treat unchanged done counts as normal progress"
    );
    assert!(
        component.contains("this._runState.error && this._runState.error.message"),
        "setup component must render backend action error messages instead of hiding them behind a generic timeout"
    );
    assert!(
        component.contains("_staleManagedAction(action)")
            && component.contains("this._oauthComplete(action.oauthKind || this._oauthKind())"),
        "setup component retry must not replay stale Microsoft device-login actions after OAuth has completed and must support non-Graph OAuth steps"
    );
    assert!(
        component.contains("step === \"bot_framework_endpoint_registration\"")
            && component.contains("step === \"teams_app_publish\"")
            && component.contains("step === \"teams_app_user_install\""),
        "setup component outcome messages must recognize generic backend-contract step ids"
    );

    let backend_contract: Value = serde_json::from_slice(&fs::read(
        source_dir.join("assets/setup/backend-contract.json"),
    )?)?;
    assert_eq!(
        backend_contract.get("schema_id").and_then(Value::as_str),
        Some("greentic.setup.backend-contract.v1")
    );
    let server_owned = backend_contract
        .get("server_owned_config_keys")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("setup backend contract missing server_owned_config_keys"))?;
    for expected in [
        "oauth_kind",
        "oauth_device_code",
        "oauth_user_code",
        "azure_management_device_code",
        "azure_management_user_code",
        "graph_access_token",
        "azure_management_access_token",
    ] {
        assert!(
            server_owned
                .iter()
                .any(|item| item.as_str() == Some(expected)),
            "setup backend contract missing server-owned key {expected}"
        );
    }
    let order = backend_contract
        .get("required_order")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("setup backend contract missing required_order"))?;
    let expected_order = [
        "graph_admin_consent",
        "bot_app_identity",
        "microsoft_bot_channel_registration_consent",
        "bot_framework_endpoint_registration",
        "teams_app_publish",
        "teams_app_user_install",
        "first_bot_framework_post",
    ];
    let actual_order: Vec<_> = order.iter().filter_map(Value::as_str).collect();
    assert_eq!(
        actual_order, expected_order,
        "setup backend contract required_order must exactly match the runtime/UI setup steps"
    );
    let position = |name: &str| {
        order
            .iter()
            .position(|item| item.as_str() == Some(name))
            .ok_or_else(|| anyhow!("setup backend contract missing required order step {name}"))
    };
    assert!(
        position("bot_app_identity")? < position("microsoft_bot_channel_registration_consent")?
            && position("microsoft_bot_channel_registration_consent")?
                < position("bot_framework_endpoint_registration")?,
        "setup backend contract must authorize Microsoft bot channel registration before Bot Framework endpoint registration"
    );
    assert!(
        position("bot_framework_endpoint_registration")? < position("teams_app_publish")?,
        "setup backend contract must require Bot Framework endpoint registration before Teams app publishing"
    );
    assert_eq!(
        backend_contract
            .get("actions_schema_id")
            .and_then(Value::as_str),
        Some("greentic.setup.actions.v1"),
        "setup backend contract must declare the generic setup action schema"
    );
    let actions = backend_contract
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("setup backend contract missing executable actions"))?;
    let mut actions_by_id = BTreeMap::new();
    for action in actions {
        let id = action
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("setup action missing id"))?;
        let executor = action
            .get("executor")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("setup action {id} missing executor"))?;
        let kind = executor
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("setup action {id} missing executor kind"))?;
        assert!(
            !kind.trim().is_empty(),
            "setup action {id} executor kind must not be empty"
        );
        actions_by_id.insert(id, action);
    }
    for step in order.iter().filter_map(Value::as_str) {
        assert!(
            actions_by_id.contains_key(step),
            "setup backend contract required step {step} must declare an executable action"
        );
    }
    assert_eq!(
        actions_by_id
            .get("graph_admin_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("oauth_device_code"),
        "Graph setup must declare generic device-code OAuth execution"
    );
    assert_eq!(
        actions_by_id
            .get("graph_admin_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("client_id_default"))
            .and_then(Value::as_str),
        Some("14d82eec-204b-4c2f-b7e8-296a70dab67e"),
        "Graph setup must provide a public device-flow client default so one-button setup does not require manual graph_setup_client_id entry"
    );
    assert_eq!(
        actions_by_id
            .get("graph_admin_consent")
            .and_then(|action| action.get("completion"))
            .and_then(|completion| completion.get("state_path"))
            .and_then(Value::as_str),
        Some("oauth.graph.ok"),
        "Graph setup completion path must be relative to the backend values object used by greentic-setup"
    );
    let graph_scopes = actions_by_id
        .get("graph_admin_consent")
        .and_then(|action| action.get("executor"))
        .and_then(|executor| executor.get("scopes"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Graph setup action missing scopes"))?;
    assert!(
        graph_scopes.iter().any(|scope| scope.as_str()
            == Some("https://graph.microsoft.com/TeamsAppInstallation.ReadWriteForUser")),
        "Graph setup must request the delegated Teams user app install scope"
    );
    assert!(
        !graph_scopes.iter().any(|scope| {
            scope.as_str()
                == Some("https://graph.microsoft.com/TeamsAppInstallation.ReadWriteForUser.All")
        }),
        "Graph device-code setup must not request application-only TeamsAppInstallation.ReadWriteForUser.All"
    );
    assert_eq!(
        actions_by_id
            .get("microsoft_bot_channel_registration_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("oauth_device_code"),
        "Microsoft bot channel registration must use generic device-code OAuth execution"
    );
    assert_eq!(
        actions_by_id
            .get("microsoft_bot_channel_registration_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("oauth_kind"))
            .and_then(Value::as_str),
        Some("management"),
        "Microsoft bot channel registration must use management OAuth state"
    );
    let management_scopes = actions_by_id
        .get("microsoft_bot_channel_registration_consent")
        .and_then(|action| action.get("executor"))
        .and_then(|executor| executor.get("scopes"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Microsoft bot channel registration action missing scopes"))?;
    assert_eq!(
        actions_by_id
            .get("graph_admin_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("device_code_store_key"))
            .and_then(Value::as_str),
        Some("oauth_device_code"),
        "Graph device OAuth must keep the legacy device-code key"
    );
    assert_eq!(
        actions_by_id
            .get("graph_admin_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("user_code_store_key"))
            .and_then(Value::as_str),
        Some("oauth_user_code"),
        "Graph device OAuth must keep the legacy user-code key used by the setup UI"
    );
    assert_eq!(
        actions_by_id
            .get("microsoft_bot_channel_registration_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("device_code_store_key"))
            .and_then(Value::as_str),
        Some("azure_management_device_code"),
        "Azure management device OAuth must not overwrite the Graph device code"
    );
    assert_eq!(
        actions_by_id
            .get("microsoft_bot_channel_registration_consent")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("user_code_store_key"))
            .and_then(Value::as_str),
        Some("azure_management_user_code"),
        "Azure management device OAuth must not overwrite the Graph device user code"
    );
    assert!(
        management_scopes
            .iter()
            .any(|scope| scope.as_str() == Some("https://management.azure.com/user_impersonation")),
        "Microsoft bot channel registration must request Azure management delegated scope"
    );
    assert_eq!(
        actions_by_id
            .get("bot_framework_endpoint_registration")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("provider_http"),
        "Bot endpoint setup must declare generic provider-owned HTTP execution"
    );
    assert_eq!(
        actions_by_id
            .get("bot_framework_endpoint_registration")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("path_template"))
            .and_then(Value::as_str),
        Some("/v1/setup/messaging-teams/{tenant}/{team}/bot-framework-registration"),
        "Bot endpoint setup must declare the provider-owned registration path template"
    );
    assert_eq!(
        actions_by_id
            .get("bot_framework_endpoint_registration")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("body"))
            .and_then(|body| body.get("messaging_endpoint"))
            .and_then(Value::as_str),
        Some("{public_base_url}/v1/messaging/ingress/messaging-teams/{tenant}/{team}"),
        "Bot endpoint setup must send the runtime messaging endpoint in the provider HTTP body"
    );
    assert_eq!(
        actions_by_id
            .get("bot_framework_endpoint_registration")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("body"))
            .and_then(|body| body.get("public_base_url"))
            .and_then(Value::as_str),
        Some("{public_base_url}"),
        "Bot endpoint setup must pass public_base_url to the provider-owned setup op"
    );
    let bot_registration_body = actions_by_id
        .get("bot_framework_endpoint_registration")
        .and_then(|action| action.get("executor"))
        .and_then(|executor| executor.get("body"))
        .ok_or_else(|| anyhow!("Bot endpoint setup missing provider HTTP body"))?;
    for expected in [
        "azure_management_access_token",
        "azure_auth_tenant",
        "azure_subscription_id",
        "azure_resource_group",
        "azure_resource_group_location",
        "azure_location",
        "azure_bot_name",
    ] {
        assert!(
            bot_registration_body.get(expected).is_some(),
            "Bot endpoint setup must pass Microsoft channel registration field {expected}"
        );
    }
    let build_pack = fs::read_to_string(source_dir.join("build_pack.sh"))?;
    assert!(
        build_pack.contains("id: messaging-teams-bot-framework-registration")
            && build_pack.contains(
                "pattern: /v1/setup/messaging-teams/{{tenant}}/{{team}}/bot-framework-registration"
            )
            && build_pack.contains("setup_component_ref: messaging-ingress-teams")
            && build_pack.contains("setup_op: bot-framework-registration"),
        "generated pack must declare the provider_http setup route with setup_component_ref/setup_op"
    );
    assert_eq!(
        actions_by_id
            .get("bot_framework_endpoint_registration")
            .and_then(|action| action.get("completion"))
            .and_then(|completion| completion.get("state_path"))
            .and_then(Value::as_str),
        Some("last_reconcile"),
        "Bot endpoint completion path must be relative to backend values, not prefixed with values."
    );
    assert_eq!(
        actions_by_id
            .get("teams_app_publish")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("provider_http"),
        "Teams app publishing must use provider-owned HTTP execution so the pack can reuse existing catalog versions"
    );
    assert_eq!(
        actions_by_id
            .get("teams_app_publish")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("path_template"))
            .and_then(Value::as_str),
        Some("/v1/setup/messaging-teams/{tenant}/{team}/teams-app-publish"),
        "Teams app publishing must declare the provider-owned publish path template"
    );
    assert_eq!(
        actions_by_id
            .get("teams_app_user_install")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("provider_http"),
        "Teams app install must use provider-owned HTTP execution so it can resolve reused catalog apps"
    );
    assert_eq!(
        actions_by_id
            .get("teams_app_user_install")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("path_template"))
            .and_then(Value::as_str),
        Some("/v1/setup/messaging-teams/{tenant}/{team}/teams-app-install"),
        "Teams app install must declare the provider-owned install path template"
    );
    assert!(
        build_pack.contains("id: messaging-teams-app-publish")
            && build_pack.contains(
                "pattern: /v1/setup/messaging-teams/{{tenant}}/{{team}}/teams-app-publish"
            )
            && build_pack.contains("setup_op: teams-app-publish")
            && build_pack.contains("id: messaging-teams-app-install")
            && build_pack.contains(
                "pattern: /v1/setup/messaging-teams/{{tenant}}/{{team}}/teams-app-install"
            )
            && build_pack.contains("setup_op: teams-app-install"),
        "generated pack must declare provider_http setup routes for Teams app publish/install"
    );
    assert_eq!(
        actions_by_id
            .get("first_bot_framework_post")
            .and_then(|action| action.get("executor"))
            .and_then(|executor| executor.get("kind"))
            .and_then(Value::as_str),
        Some("runtime_observation"),
        "final setup step must be driven by a runtime observation, not manual Teams install alone"
    );

    let setup_routes_entry = answer
        .get("pack_overlay")
        .and_then(|value| value.get("files"))
        .and_then(Value::as_array)
        .and_then(|files| {
            files.iter().find(|file| {
                file.get("path").and_then(Value::as_str) == Some("assets/setup.routes.json")
            })
        })
        .ok_or_else(|| anyhow!("missing assets/setup.routes.json overlay"))?;
    let setup_routes: Value = serde_json::from_str(
        setup_routes_entry
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("setup.routes.json overlay missing content"))?,
    )?;
    assert_eq!(
        setup_routes.get("schema_id").and_then(Value::as_str),
        Some("greentic.setup.web-component.v1")
    );
    assert_eq!(
        setup_routes.get("tag_name").and_then(Value::as_str),
        Some("greentic-teams-setup-v4")
    );
    assert_eq!(
        setup_routes.get("module_url").and_then(Value::as_str),
        Some(expected_module_url.as_str())
    );
    assert_eq!(
        setup_routes
            .get("events")
            .and_then(|value| value.get("complete"))
            .and_then(Value::as_str),
        Some("greentic-provider-setup-complete")
    );

    let manifest: Value = serde_json::from_slice(&fs::read(
        source_dir.join("assets/teams-app/manifest.template.json"),
    )?)?;
    assert!(
        manifest.get("packageName").is_none(),
        "Teams manifest template must not include unsupported packageName"
    );
    assert!(
        manifest
            .get("bots")
            .and_then(Value::as_array)
            .is_some_and(|bots| !bots.is_empty()),
        "Teams manifest template must declare a bot"
    );
    let manifest_text =
        fs::read_to_string(source_dir.join("assets/teams-app/manifest.template.json"))?;
    assert!(
        manifest_text.contains("{bot_app_id}")
            && manifest_text.contains("{teams_app_id}")
            && manifest_text.contains("{teams_app_version}")
            && !manifest_text.contains("{{ bot_app_id }}")
            && !manifest_text.contains("{{ teams_app_id }}"),
        "Teams manifest template must use greentic-setup placeholder syntax"
    );
    for icon in ["assets/teams-app/color.png", "assets/teams-app/outline.png"] {
        assert!(
            source_dir.join(icon).is_file(),
            "Teams manifest references {icon}, so the source pack must include it"
        );
        assert!(
            answer
                .get("assets")
                .and_then(Value::as_array)
                .is_some_and(|assets| assets.iter().any(|asset| asset.as_str() == Some(icon))),
            "Teams build-answer assets must package {icon}"
        );
    }

    let build_pack = fs::read_to_string(source_dir.join("build_pack.sh"))?;
    assert!(
        build_pack.contains("greentic.ext.capabilities.v1")
            && build_pack.contains("greentic.cap.messaging.provider.v1")
            && build_pack.contains("messaging-teams-v1"),
        "messaging-teams pack build must advertise canonical provider capabilities"
    );
    assert!(
        build_pack.contains("Do not also declare them under `assets:`"),
        "messaging-teams build must avoid duplicating static-route assets as assets/assets/*"
    );
    assert!(
        build_pack.contains("greentic.setup.backend-contract.v1"),
        "messaging-teams build must advertise the provider setup backend contract"
    );
    assert!(
        build_pack.contains("greentic.provider-extension.v1"),
        "messaging-teams ships a Bot Framework egress connector, so its pack must declare the schema-core provider extension"
    );

    Ok(())
}

#[test]
fn messaging_teams_setup_conformance_covers_required_states() -> Result<()> {
    let root = workspace_root();
    let conformance_path = root
        .join("messaging-teams")
        .join("assets/setup/conformance.json");
    let conformance: Value = serde_json::from_slice(&fs::read(&conformance_path)?)?;

    assert_eq!(
        conformance.get("provider_id").and_then(Value::as_str),
        Some("messaging-teams")
    );
    let states = conformance
        .get("setup_states")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing setup_states"))?;
    for expected in [
        "fresh_setup",
        "graph_device_login_start",
        "graph_device_login_pending",
        "graph_device_login_expired",
        "graph_device_login_refresh_code",
        "microsoft_bot_channel_registration_consent",
        "bot_framework_registration_before_publish",
        "teams_app_publish",
        "teams_app_install",
        "open_bot_chat",
        "first_activity_received",
        "send_test_card",
        "adaptive_card_action_received",
    ] {
        assert!(
            states.iter().any(|state| state.as_str() == Some(expected)),
            "missing setup conformance state {expected}"
        );
    }

    let runtime_cases = conformance
        .get("runtime_cases")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing runtime_cases"))?;
    for expected in [
        "bot_framework_activity_ingestion",
        "bot_framework_authentication_failure",
        "bot_framework_message_normalization",
        "adaptive_card_submit_normalization",
        "adaptive_card_follow_up_response",
        "conversation_context_available",
    ] {
        assert!(
            runtime_cases
                .iter()
                .any(|case| case.as_str() == Some(expected)),
            "missing runtime conformance case {expected}"
        );
    }

    let tester = conformance
        .get("tester_contract")
        .ok_or_else(|| anyhow!("missing tester_contract"))?;
    let setup_component = conformance
        .get("setup_component_contract")
        .ok_or_else(|| anyhow!("missing setup_component_contract"))?;
    let setup_backend = conformance
        .get("setup_backend_contract")
        .ok_or_else(|| anyhow!("missing setup_backend_contract"))?;
    assert_eq!(
        setup_backend
            .get("actions_schema_id")
            .and_then(Value::as_str),
        Some("greentic.setup.actions.v1")
    );
    let required_action_kinds = setup_backend
        .get("required_action_kinds")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing setup_backend_contract.required_action_kinds"))?;
    for expected in [
        "oauth_device_code",
        "microsoft_graph_application",
        "provider_http",
        "runtime_observation",
    ] {
        assert!(
            required_action_kinds
                .iter()
                .any(|kind| kind.as_str() == Some(expected)),
            "missing setup backend action kind {expected}"
        );
    }
    assert_eq!(
        setup_component.get("schema_id").and_then(Value::as_str),
        Some("greentic.setup.web-component.v1")
    );
    assert_eq!(
        setup_component
            .get("completion_state_path")
            .and_then(Value::as_str),
        Some("setup_status.ok")
    );
    let generic_events = setup_component
        .get("generic_events")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing setup_component_contract.generic_events"))?;
    assert!(
        generic_events
            .iter()
            .any(|event| event.as_str() == Some("greentic-provider-setup-complete")),
        "setup component conformance must declare generic completion event"
    );
    assert_eq!(
        tester
            .get("node_botbuilder_sidecar_default")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        tester
            .get("azure_bot_service_required")
            .and_then(Value::as_bool),
        Some(true)
    );

    Ok(())
}

#[test]
fn webchat_gui_pack_contains_runtime_bootstrap_and_bundled_assets() -> Result<()> {
    let root = workspace_root();
    let asset_root = root
        .join("packs")
        .join("messaging-webchat-gui")
        .join("assets/webchat-gui");

    for rel in [
        "index.html",
        "404.html",
        "runtime-bootstrap.js",
        "embed.js",
        "config/product.json",
        "greentic-sso.js",
        "sso-callback.html",
    ] {
        let path = asset_root.join(rel);
        assert!(path.exists(), "missing asset {}", path.display());
    }

    let bootstrap = fs::read_to_string(asset_root.join("runtime-bootstrap.js"))?;
    assert!(
        bootstrap.contains("/v1/web/webchat/"),
        "runtime bootstrap should resolve tenant from the GUI path"
    );
    assert!(
        bootstrap.contains("/v1/messaging/webchat/"),
        "runtime bootstrap should point to provider-scoped backend routes"
    );
    assert!(
        bootstrap.contains("__WEBCHAT_BACKEND_BASE__"),
        "runtime bootstrap should expose the backend base"
    );

    let embed = fs::read_to_string(asset_root.join("embed.js"))?;
    assert!(
        embed.contains("customElements.define(\"greentic-webchat\""),
        "embed.js should define the greentic-webchat custom element"
    );
    assert!(
        embed.contains("presentation_mode\", \"embed_webcomponent\""),
        "embed.js should request embedded presentation mode"
    );

    let pack_yaml = fs::read_to_string(
        root.join("packs")
            .join("messaging-webchat-gui")
            .join("pack.yaml"),
    )?;
    assert!(
        pack_yaml.contains("assets/webchat-gui/embed.js"),
        "pack.yaml should include embed.js as a static asset"
    );
    assert!(
        pack_yaml.contains("assets/webchat-gui/greentic-sso.js"),
        "pack.yaml must declare the bundled SSO SDK asset"
    );

    let mut has_js_bundle = false;
    let mut has_css_bundle = false;
    for entry in fs::read_dir(asset_root.join("assets"))? {
        let path = entry?.path();
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            has_js_bundle |= name.ends_with(".js");
            has_css_bundle |= name.ends_with(".css");
        }
    }
    assert!(has_js_bundle, "expected packaged JS bundle");
    assert!(has_css_bundle, "expected packaged CSS bundle");

    Ok(())
}

/// `greentic-setup` scaffolds every per-tenant config from `default.json`
/// (see `resolve_or_scaffold_tenant_config`). If the pack ships no
/// `default.json`, the scaffold step returns `None` and `gtc setup` silently
/// drops the operator's skin, nav_links, and OAuth answers. Guard against the
/// template being deleted again (it was removed in 3f1535ad).
#[test]
fn webchat_gui_pack_ships_default_tenant_config() -> Result<()> {
    let root = workspace_root();
    let default_tenant = root
        .join("packs")
        .join("messaging-webchat-gui")
        .join("assets/webchat-gui/config/tenants/default.json");

    assert!(
        default_tenant.exists(),
        "missing scaffold template {} — greentic-setup needs it to scaffold per-tenant configs",
        default_tenant.display()
    );

    let config: Value = serde_json::from_slice(&fs::read(&default_tenant)?)?;
    assert_eq!(
        config.get("tenant_id").and_then(Value::as_str),
        Some("default"),
        "default.json must declare tenant_id=default"
    );
    assert!(
        config
            .get("skin")
            .and_then(Value::as_str)
            .is_some_and(|skin| !skin.is_empty()),
        "default.json must declare a non-empty skin so scaffolded tenants have a valid theme"
    );
    assert!(
        config.get("nav_links").is_none(),
        "default.json is a neutral template — nav_links come from sync_nav_links_to_tenant_config"
    );
    let enabled_dummy_login = config
        .get("auth")
        .and_then(|auth| auth.get("providers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|provider| {
            provider.get("type").and_then(Value::as_str) == Some("dummy")
                && provider
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        });
    assert!(
        !enabled_dummy_login,
        "default.json must not enable dummy login by default"
    );

    Ok(())
}

#[test]
fn webchat_gui_config_schema_declares_presentation_mode() -> Result<()> {
    let root = workspace_root();
    let schema_path = root
        .join("packs")
        .join("messaging-webchat-gui")
        .join("schemas")
        .join("messaging")
        .join("webchat-gui")
        .join("public.config.schema.json");
    let schema: Value = serde_json::from_slice(&fs::read(schema_path)?)?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("webchat-gui public config schema missing properties"))?;
    let presentation_mode = properties
        .get("presentation_mode")
        .ok_or_else(|| anyhow!("webchat-gui public config schema missing presentation_mode"))?;

    assert_eq!(
        presentation_mode.get("default").and_then(Value::as_str),
        Some("standalone")
    );
    let allowed = presentation_mode
        .get("enum")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("presentation_mode missing enum"))?;
    assert!(
        allowed
            .iter()
            .any(|value| value.as_str() == Some("standalone")),
        "presentation_mode schema should allow standalone"
    );
    assert!(
        allowed
            .iter()
            .any(|value| value.as_str() == Some("embed_webcomponent")),
        "presentation_mode schema should allow embed_webcomponent"
    );
    assert!(
        properties.contains_key("skin"),
        "webchat-gui schema should keep skin as visual theme"
    );
    assert_eq!(
        properties
            .get("text_input_enabled")
            .and_then(|property| property.get("default"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        properties.contains_key("nav_links"),
        "webchat-gui schema should keep nav_links for standalone mode"
    );

    Ok(())
}
