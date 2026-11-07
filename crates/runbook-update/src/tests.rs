use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

fn manifest() -> Manifest {
    Manifest {
        schema: 1,
        app: "dev.runbook.desktop".into(),
        version: "0.2.0".into(),
        platform: "windows-x86_64".into(),
        data_schema: 1,
        url: "https://example.com/update.zip".into(),
        size: 1,
        sha256: "00".repeat(32),
        notes: "修复终端与更新功能".into(),
    }
}
fn signed(m: &Manifest) -> (Vec<u8>, String) {
    let key = SigningKey::from_bytes(&[47; 32]);
    let payload = serde_json::to_string(m).unwrap();
    let signature = STANDARD.encode(key.sign(payload.as_bytes()).to_bytes());
    (
        serde_json::to_vec(&serde_json::json!({"payload":payload,"signature":signature})).unwrap(),
        STANDARD.encode(key.verifying_key().to_bytes()),
    )
}

#[test]
fn manifest_requires_authentic_metadata_and_compatible_schema() {
    let (bytes, public) = signed(&manifest());
    assert_eq!(verify_manifest(&bytes, &public).unwrap().version, "0.2.0");
    let tampered = String::from_utf8(bytes).unwrap().replace("0.2.0", "9.9.9");
    assert!(verify_manifest(tampered.as_bytes(), &public).is_err());
    for m in [
        Manifest {
            data_schema: 2,
            ..manifest()
        },
        Manifest {
            app: "other".into(),
            ..manifest()
        },
        Manifest {
            url: "http://example.com/update.zip".into(),
            ..manifest()
        },
        Manifest {
            size: u64::MAX,
            ..manifest()
        },
        Manifest {
            version: "0.2.0-beta".into(),
            ..manifest()
        },
    ] {
        let (bytes, public) = signed(&m);
        assert!(verify_manifest(&bytes, &public).is_err());
    }
}

#[test]
fn stable_versions_use_semver_and_reject_downgrades() {
    assert!(manifest::newer("1.0.0").unwrap());
    assert!(!manifest::newer(CURRENT_VERSION).unwrap());
    assert!(!manifest::newer("0.0.1").unwrap());
    assert!(manifest::newer("broken").is_err());
}

#[test]
fn zip_paths_cannot_touch_data_escape_or_alias_windows_devices() {
    for name in [
        "../runbook.exe",
        "docs/../../data/runbook.db",
        "data/runbook.db",
        "C:/runbook.exe",
        "/runbook.exe",
        "docs/evil:stream",
        "docs/CON.txt",
        "scripts/NUL",
        "docs/foo.",
        "docs/a ",
        "docs\\a",
        "docs//a",
        "docs/./a",
    ] {
        assert!(!package::safe_name(name), "{name}");
    }
    for name in [
        "Runbook-Desktop.exe",
        "runbook.exe",
        "runbook-updater.exe",
        "docs/CLI.md",
        "skills/runbook-cli/SKILL.md",
    ] {
        assert!(package::safe_name(name), "{name}");
    }
}

fn archive(path: &std::path::Path, names: &[&str]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for name in names {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"binary placeholder").unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn extraction_requires_complete_pair_and_rejects_duplicate_or_untrusted_files() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("package.zip");
    for names in [
        vec!["runbook.exe"],
        vec![
            "Runbook-Desktop.exe",
            "runbook.exe",
            "runbook-updater.exe",
            "data/runbook.db",
        ],
        vec![
            "Runbook-Desktop.exe",
            "runbook.exe",
            "runbook-updater.exe",
            "RUNBOOK.EXE",
        ],
    ] {
        archive(&zip, &names);
        let out = tempfile::tempdir().unwrap();
        assert!(package::extract(&zip, out.path()).is_err());
    }
    archive(&zip, &package::REQUIRED);
    let out = tempfile::tempdir().unwrap();
    assert_eq!(package::extract(&zip, out.path()).unwrap().len(), 3);
}

#[test]
fn package_hash_and_length_detect_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("package.zip");
    fs::write(&path, b"correct").unwrap();
    let m = Manifest {
        size: 7,
        sha256: format!("{:x}", Sha256::digest(b"correct")),
        ..manifest()
    };
    package::verify_archive(&path, &m).unwrap();
    fs::write(&path, b"corrupt").unwrap();
    assert!(package::verify_archive(&path, &m).is_err());
    fs::write(&path, b"short").unwrap();
    assert!(package::verify_archive(&path, &m).is_err());
}

#[test]
fn replacement_changes_file_without_removing_unrelated_data() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("runbook.exe");
    let source = dir.path().join("new");
    fs::write(&target, b"old").unwrap();
    fs::write(&source, b"new").unwrap();
    fs::write(dir.path().join("user-data"), b"keep").unwrap();
    package::replace(&source, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(fs::read(dir.path().join("user-data")).unwrap(), b"keep");
}

#[test]
fn runtime_locks_allow_parallel_cli_but_block_file_replacement() {
    use fs2::FileExt;
    let dir = tempfile::tempdir().unwrap();
    let _a = RuntimeGuard::acquire(dir.path()).unwrap();
    let _b = RuntimeGuard::acquire(dir.path()).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join(".runbook-runtime.lock"))
        .unwrap();
    assert!(FileExt::try_lock_exclusive(&file).is_err());
}
