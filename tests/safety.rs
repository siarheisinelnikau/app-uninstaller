use app_uninstaller::{
    command,
    inventory::InstalledApp,
    leftovers::{self, SafetyPolicy},
    settings,
};
use std::{fs, path::PathBuf};

fn fixture_dir() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("safety-")
        .tempdir_in(root)
        .unwrap()
}

fn app(command: &str) -> InstalledApp {
    InstalledApp {
        id: "fixture/user/Example".into(),
        name: "Example App".into(),
        uninstall_string: command.into(),
        ..Default::default()
    }
}

#[test]
fn command_planning_never_uses_a_shell_or_ambiguous_path() {
    for unsafe_command in [
        "cmd.exe /c del anything",
        "C:\\Windows\\System32\\cmd.exe /c echo hi",
        "\"C:\\Windows\\System32\\rundll32.exe\" something.dll,Entry",
        "C:\\Program Files\\Example\\uninstall.exe /remove",
        "\\\\server\\share\\remove.exe",
        "relative.exe",
        "C:\\a\\..\\remove.exe",
        "C:\\remove.exe:stream.exe",
        "\"C:\\remove.exe\"/s",
        "\"C:\\uninstall.exe",
        "C:\\remove.exe\n/s",
        "C:\\remove.bat",
        "C:\\remove.exe\0",
    ] {
        assert!(
            command::parse_registered(unsafe_command).is_err(),
            "accepted {unsafe_command:?}"
        );
    }
    let plan = command::parse_registered(
        "\"C:\\Program Files\\Example\\uninstall.exe\" /remove \"a & b\"",
    )
    .unwrap();
    assert_eq!(
        plan.executable,
        PathBuf::from("C:\\Program Files\\Example\\uninstall.exe")
    );
    assert_eq!(plan.arguments, "/remove \"a & b\"");
}

#[test]
fn msi_removal_uses_only_the_registered_product_code() {
    let mut fixture = app("MsiExec.exe /I{00000000-1111-2222-3333-444444444444}");
    fixture.windows_installer = true;
    fixture.key_name = "{00000000-1111-2222-3333-444444444444}".into();
    let plan = command::plan(&fixture, "C:\\Windows").unwrap();
    assert_eq!(
        plan.arguments,
        "/x {00000000-1111-2222-3333-444444444444} /norestart"
    );
    assert!(!command::valid_product_code(
        "{00000000-1111-2222-3333-44444444444Z}"
    ));
    fixture.key_name.push_str(" /quiet");
    assert!(command::plan(&fixture, "C:\\Windows").is_err());
}

#[test]
fn search_and_optional_inventory_metadata_work() {
    let mut fixture = app("");
    fixture.publisher = "Independent Studio".into();
    assert!(fixture.matches("STUDIO"));
    assert!(fixture.matches("example"));
    assert!(!fixture.matches("missing"));
    assert!(fixture.estimated_kib.is_none());
}

#[cfg(windows)]
fn setup() -> (tempfile::TempDir, InstalledApp, SafetyPolicy) {
    let temp = fixture_dir();
    let root = temp.path().join("Example App");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("uninstall.exe"),
        b"inert fixture; never executable",
    )
    .unwrap();
    fs::write(root.join("settings.dat"), b"fixture settings").unwrap();
    let fixture = InstalledApp {
        install_location: Some(root.clone()),
        ..app(&format!(
            "\"{}\" /remove",
            root.join("uninstall.exe").display()
        ))
    };
    let policy = SafetyPolicy {
        allowed_parents: vec![temp.path().into()],
        excluded_roots: vec![],
    };
    (temp, fixture, policy)
}

#[cfg(windows)]
#[test]
fn quarantine_and_restore_preserve_fixture_bytes_and_refuse_overwrite() {
    let (temp, fixture, policy) = setup();
    let snapshot = leftovers::capture(&fixture, &[fixture.clone()], &policy).unwrap();
    assert!(leftovers::review(&snapshot, &[fixture.clone()], &policy).is_err());
    fs::remove_file(snapshot.root.join("uninstall.exe")).unwrap();
    let candidate = leftovers::review(&snapshot, &[], &policy).unwrap().unwrap();
    assert_eq!(candidate.files.len(), 1);
    let record = leftovers::quarantine(&candidate, &temp.path().join("journal")).unwrap();
    assert!(!snapshot.root.exists());
    assert_eq!(
        fs::read(record.backup.join("settings.dat")).unwrap(),
        b"fixture settings"
    );
    assert_eq!(
        leftovers::records(&temp.path().join("journal"))
            .unwrap()
            .len(),
        1
    );
    fs::create_dir(&record.original).unwrap();
    assert!(leftovers::restore(&record).is_err());
    fs::remove_dir(&record.original).unwrap();
    leftovers::restore(&record).unwrap();
    assert_eq!(
        fs::read(snapshot.root.join("settings.dat")).unwrap(),
        b"fixture settings"
    );
    assert!(
        leftovers::records(&temp.path().join("journal"))
            .unwrap()
            .is_empty()
    );
}

#[cfg(windows)]
#[test]
fn ownership_rejects_overlap_protected_and_weak_evidence() {
    let (_temp, fixture, mut policy) = setup();
    let mut other = fixture.clone();
    other.id = "different-registration".into();
    assert!(leftovers::capture(&fixture, &[fixture.clone(), other], &policy).is_err());
    policy
        .excluded_roots
        .push(fixture.install_location.clone().unwrap());
    assert!(leftovers::capture(&fixture, &[], &policy).is_err());
    policy.excluded_roots.clear();
    let mut mismatch = fixture.clone();
    mismatch.name = "Another Product".into();
    assert!(leftovers::capture(&mismatch, &[], &policy).is_err());
    mismatch = fixture.clone();
    mismatch.uninstall_string.clear();
    assert!(leftovers::capture(&mismatch, &[], &policy).is_err());
    mismatch = fixture.clone();
    mismatch.system_component = true;
    assert!(leftovers::capture(&mismatch, &[], &policy).is_err());
}

#[cfg(windows)]
#[test]
fn changed_or_new_files_and_stale_previews_block_quarantine() {
    let (temp, fixture, policy) = setup();
    let snapshot = leftovers::capture(&fixture, &[], &policy).unwrap();
    let candidate = leftovers::review(&snapshot, &[], &policy).unwrap().unwrap();
    fs::write(snapshot.root.join("settings.dat"), b"changed").unwrap();
    assert!(leftovers::review(&snapshot, &[], &policy).is_err());
    assert!(leftovers::quarantine(&candidate, &temp.path().join("journal")).is_err());
    fs::write(snapshot.root.join("settings.dat"), b"fixture settings").unwrap();
    fs::write(snapshot.root.join("new-user-file.txt"), b"new").unwrap();
    assert!(leftovers::review(&snapshot, &[], &policy).is_err());
    assert!(snapshot.root.exists());
}

#[test]
fn settings_opt_out_is_persistent_and_corruption_is_visible() {
    let temp = fixture_dir();
    assert!(settings::load(temp.path()).unwrap().check_on_startup);
    settings::save(
        temp.path(),
        &settings::Settings {
            check_on_startup: false,
        },
    )
    .unwrap();
    assert!(!settings::load(temp.path()).unwrap().check_on_startup);
    fs::write(temp.path().join("settings.json"), b"bad json").unwrap();
    assert!(settings::load(temp.path()).is_err());
}

#[test]
fn manifest_hashes_are_stable_and_read_only() {
    let temp = fixture_dir();
    fs::create_dir(temp.path().join("nested")).unwrap();
    fs::write(temp.path().join("nested/a.txt"), b"abc").unwrap();
    let result = leftovers::manifest(temp.path()).unwrap();
    assert_eq!(
        result[&PathBuf::from("nested/a.txt")].sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
