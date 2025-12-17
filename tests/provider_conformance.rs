use std::fs;
use std::path::Path;

const EXPECTED_EXPORTS: &[&str] = &[
    "export send-message",
    "export handle-webhook",
    "export refresh",
    "export format-message",
];

#[test]
fn components_have_manifests_and_exports() {
    let components_dir = Path::new("components");
    for entry in fs::read_dir(components_dir).expect("components dir readable") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();

        // manifest must exist and contain secret_requirements (may be empty).
        let manifest_path = entry.path().join("component.manifest.json");
        assert!(
            manifest_path.exists(),
            "missing manifest for component {name}"
        );
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&manifest_path).expect("manifest read"),
        )
        .expect("manifest json");
        assert!(
            manifest.get("secret_requirements").is_some(),
            "manifest missing secret_requirements for {name}"
        );

        // world.wit must export the expected functions.
        let world_wit = entry.path().join("wit").join(&*name).join("world.wit");
        assert!(
            world_wit.exists(),
            "missing world.wit for component {name} at {world_wit:?}"
        );
        let contents = fs::read_to_string(&world_wit).expect("read world.wit");
        for export in EXPECTED_EXPORTS {
            assert!(
                contents.contains(export),
                "component {name} world.wit missing export {export}"
            );
        }
    }
}

#[test]
fn components_do_not_use_env_vars() {
    let components_dir = Path::new("components");
    for entry in fs::read_dir(components_dir).expect("components dir readable") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let src_dir = entry.path().join("src");
        if !src_dir.exists() {
            continue;
        }
        for src_entry in fs::read_dir(&src_dir).expect("src dir readable") {
            let src_entry = src_entry.expect("src entry");
            if !src_entry.file_type().expect("file type").is_file() {
                continue;
            }
            let contents = fs::read_to_string(src_entry.path()).expect("read src");
            assert!(
                !contents.contains("std::env") && !contents.contains("env!"),
                "env usage found in {}",
                src_entry.path().display()
            );
        }
    }
}
