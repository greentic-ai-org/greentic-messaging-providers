//! Build the Teams app package (.zip) in-component from embedded assets, so
//! `teams_app_publish` can upload a real package to the Graph app catalog.
//! The component cannot read pack files at runtime, so the manifest template and
//! icons are embedded at compile time.

const MANIFEST_TEMPLATE: &str =
    include_str!("../../../messaging-teams/assets/teams-app/manifest.template.json");
const COLOR_PNG: &[u8] = include_bytes!("../../../messaging-teams/assets/teams-app/color.png");
const OUTLINE_PNG: &[u8] = include_bytes!("../../../messaging-teams/assets/teams-app/outline.png");

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn manifest(
    bot_app_id: &str,
    teams_app_id: &str,
    teams_app_version: &str,
    app_name: &str,
) -> String {
    let name = if app_name.trim().is_empty() {
        "Greentic Teams Bot"
    } else {
        app_name.trim()
    };
    let version = if teams_app_version.trim().is_empty() {
        "1.0.0"
    } else {
        teams_app_version.trim()
    };
    let escaped = json_escape(name);
    // The source template keeps the fixed name (asserted by pack_metadata tests);
    // the runtime package substitutes the operator-supplied name.
    MANIFEST_TEMPLATE
        .replace("{teams_app_id}", teams_app_id)
        .replace("{teams_app_version}", version)
        .replace("{bot_app_id}", bot_app_id)
        .replace(
            "\"short\": \"Greentic Teams Bot\"",
            &format!("\"short\": \"{escaped}\""),
        )
        .replace(
            "\"full\": \"Greentic Teams Bot\"",
            &format!("\"full\": \"{escaped}\""),
        )
}

/// Build a Teams app package zip containing manifest.json + the two icons.
pub fn build_package(
    bot_app_id: &str,
    teams_app_id: &str,
    teams_app_version: &str,
    app_name: &str,
) -> Vec<u8> {
    let manifest = manifest(bot_app_id, teams_app_id, teams_app_version, app_name);
    build_zip(&[
        ("manifest.json", manifest.as_bytes()),
        ("color.png", COLOR_PNG),
        ("outline.png", OUTLINE_PNG),
    ])
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            // mask = 0xFFFFFFFF when the low bit is set, else 0.
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Minimal ZIP writer using stored (uncompressed) entries — accepted by the
/// Teams app catalog and avoids pulling in a compression dependency.
fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<(String, u32, u32, u32)> = Vec::new(); // name, size, crc, offset

    for (name, data) in files {
        let crc = crc32(data);
        let offset = out.len() as u32;
        let size = data.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local file header sig
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0x21u16.to_le_bytes()); // mod date (1980-01-01)
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes()); // compressed size
        out.extend_from_slice(&size.to_le_bytes()); // uncompressed size
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.push((name.to_string(), size, crc, offset));
    }

    let cd_offset = out.len() as u32;
    for (name, size, crc, offset) in &central {
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes()); // central dir header sig
        out.extend_from_slice(&20u16.to_le_bytes()); // version made by
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes()); // compressed
        out.extend_from_slice(&size.to_le_bytes()); // uncompressed
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        out.extend_from_slice(&offset.to_le_bytes()); // local header offset
        out.extend_from_slice(name.as_bytes());
    }
    let cd_size = out.len() as u32 - cd_offset;

    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes()); // end of central dir sig
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&(central.len() as u16).to_le_bytes()); // entries this disk
    out.extend_from_slice(&(central.len() as u16).to_le_bytes()); // total entries
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_u32(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

    #[test]
    fn json_escape_handles_json_string_specials() {
        assert_eq!(json_escape(r#"Ops \ "Bot""#), r#"Ops \\ \"Bot\""#);
    }

    #[test]
    fn manifest_substitutes_ids_version_and_name() {
        let manifest = manifest(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            "2.3.4",
            r#"Ops "Bot""#,
        );

        assert!(manifest.contains("00000000-0000-0000-0000-000000000001"));
        assert!(manifest.contains("00000000-0000-0000-0000-000000000002"));
        assert!(manifest.contains(r#""version": "2.3.4""#));
        assert!(manifest.contains(r#""short": "Ops \"Bot\"""#));
        assert!(!manifest.contains("{bot_app_id}"));
        assert!(!manifest.contains("{teams_app_id}"));
        assert!(!manifest.contains("{teams_app_version}"));
    }

    #[test]
    fn manifest_uses_defaults_for_empty_inputs() {
        let manifest = manifest("bot-id", "teams-id", " ", " ");

        assert!(manifest.contains(r#""short": "Greentic Teams Bot""#));
        assert!(manifest.contains(r#""version": "1.0.0""#));
    }

    #[test]
    fn crc32_matches_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn build_zip_writes_stored_entries_and_directory() {
        let zip = build_zip(&[("a.txt", b"alpha"), ("b.txt", b"beta")]);

        assert_eq!(le_u32(&zip[0..4]), 0x0403_4b50);
        let text = String::from_utf8_lossy(&zip);
        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
        assert!(zip.windows(4).any(|w| w == 0x0201_4b50u32.to_le_bytes()));
        assert!(zip.windows(4).any(|w| w == 0x0605_4b50u32.to_le_bytes()));
    }

    #[test]
    fn build_package_contains_manifest_and_icon_entries() {
        let package = build_package(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            "2.3.4",
            "Ops Bot",
        );

        assert_eq!(le_u32(&package[0..4]), 0x0403_4b50);
        let text = String::from_utf8_lossy(&package);
        assert!(text.contains("manifest.json"));
        assert!(text.contains("color.png"));
        assert!(text.contains("outline.png"));
        assert!(text.contains("00000000-0000-0000-0000-000000000001"));
        assert!(text.contains("00000000-0000-0000-0000-000000000002"));
        assert!(text.contains("2.3.4"));
    }
}
