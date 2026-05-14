use std::fs;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn package_metadata_is_crates_io_ready() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("read Cargo.toml");

    assert!(manifest.contains("name = \"greentic-messaging-provider-common\""));
    assert!(manifest.contains("description = "));
    assert!(
        manifest.contains(
            "repository = \"https://github.com/greenticai/greentic-messaging-providers\""
        )
    );
    assert!(manifest.contains("readme = \"README.md\""));
    assert!(manifest.contains("license.workspace = true"));
    assert!(manifest.contains("[lib]"));
    assert!(manifest.contains("name = \"provider_common\""));
}

#[test]
fn public_documentation_files_exist() {
    for file in ["README.md", "CHANGELOG.md", "MIGRATION.md"] {
        let path = crate_root().join(file);
        let text = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("expected {} to exist: {err}", path.display());
        });
        assert!(
            text.contains("provider") || text.contains("Provider"),
            "{} should describe provider usage",
            path.display()
        );
    }
}
