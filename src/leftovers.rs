//! Conservative, bounded ownership capture and reversible directory quarantine.
//! Only the exact registered installation directory is eligible. No registry edits.
use crate::{command, inventory::InstalledApp};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 5000;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SafetyPolicy {
    pub allowed_parents: Vec<PathBuf>,
    pub excluded_roots: Vec<PathBuf>,
}
impl SafetyPolicy {
    pub fn windows() -> Self {
        let paths = |names: &[&str]| {
            names
                .iter()
                .filter_map(|n| std::env::var_os(n).map(PathBuf::from))
                .collect::<Vec<_>>()
        };
        Self {
            allowed_parents: paths(&[
                "ProgramFiles",
                "ProgramFiles(x86)",
                "LOCALAPPDATA",
                "APPDATA",
            ]),
            excluded_roots: paths(&[
                "SystemRoot",
                "PUBLIC",
                "ProgramData",
                "OneDrive",
                "OneDriveConsumer",
                "OneDriveCommercial",
            ]),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub app_id: String,
    pub app_name: String,
    pub root: PathBuf,
    pub evidence: Vec<String>,
    pub files: BTreeMap<PathBuf, Fingerprint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub snapshot: Snapshot,
    pub files: BTreeMap<PathBuf, Fingerprint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub id: String,
    pub app_name: String,
    pub original: PathBuf,
    pub backup: PathBuf,
    pub files: BTreeMap<PathBuf, Fingerprint>,
}

fn folded(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
fn within(path: &Path, root: &Path) -> bool {
    let (p, r) = (folded(path), folded(root));
    p == r || p.starts_with(&(r + "\\"))
}
fn normalized(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let meta = fs::symlink_metadata(ancestor)
            .with_context(|| format!("Cannot inspect {}", ancestor.display()))?;
        ensure!(
            !meta.file_type().is_symlink(),
            "Symbolic links are excluded: {}",
            ancestor.display()
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                meta.file_attributes() & 0x400 == 0,
                "Reparse points are excluded: {}",
                ancestor.display()
            );
        }
    }
    Ok(())
}

pub fn capture(
    app: &InstalledApp,
    inventory: &[InstalledApp],
    policy: &SafetyPolicy,
) -> Result<Snapshot> {
    ensure!(
        !app.system_component,
        "System components are excluded from leftover review"
    );
    let raw = app
        .install_location
        .as_ref()
        .context("No registered installation directory; ownership cannot be established")?;
    reject_links(raw)?;
    let root = fs::canonicalize(raw)?;
    ensure!(
        root.is_dir(),
        "Registered install location is not a directory"
    );
    let leaf = root
        .file_name()
        .context("Root directories are excluded")?
        .to_string_lossy();
    let name = normalized(&app.name);
    ensure!(
        name.len() >= 4 && normalized(&leaf) == name,
        "Installation folder name must exactly match the application name (ignoring punctuation)"
    );
    ensure!(
        ![
            "commonfiles",
            "shared",
            "windows",
            "microsoft",
            "programs",
            "documents",
            "downloads",
            "desktop",
            "pictures",
            "music",
            "videos",
            "appuninstaller"
        ]
        .contains(&name.as_str()),
        "Shared or protected folder name is excluded"
    );
    let parent = root.parent().context("No parent directory")?;
    let allowed = policy
        .allowed_parents
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .any(|p| folded(&p) == folded(parent));
    ensure!(
        allowed,
        "Only direct application folders under Program Files or AppData are eligible; documents and custom locations are excluded"
    );
    for excluded in &policy.excluded_roots {
        ensure!(
            !within(&root, excluded),
            "Protected/shared location is excluded"
        );
    }
    for other in inventory.iter().filter(|other| other.id != app.id) {
        if let Some(path) = &other.install_location {
            let path = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            ensure!(
                !within(&root, &path) && !within(&path, &root),
                "Another registered application overlaps this folder: {}",
                other.name
            );
        }
    }
    let marker = command::parse_registered(&app.uninstall_string)
        .ok()
        .map(|p| p.executable);
    let marker = marker
        .filter(|p| p.is_file())
        .and_then(|p| fs::canonicalize(p).ok());
    ensure!(
        marker.as_ref().is_some_and(|p| within(p, &root)),
        "A direct registered uninstaller inside this folder is required as ownership evidence"
    );
    let files = manifest(&root)?;
    Ok(Snapshot {
        app_id: app.id.clone(),
        app_name: app.name.clone(),
        root,
        evidence: vec![
            "Exact installation directory recorded in the uninstall registration.".into(),
            "Folder name exactly matches the registered application name.".into(),
            "Registered uninstaller was inside this folder before removal.".into(),
            "No overlapping application, shared root, document folder or reparse point was found."
                .into(),
            "Each proposed file must match its SHA-256 captured before uninstall.".into(),
        ],
        files,
    })
}

pub fn hash_file(path: &Path, max: u64) -> Result<Fingerprint> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= max, "Ownership scan exceeds its size limit");
        hash.update(&buffer[..n]);
    }
    Ok(Fingerprint {
        bytes,
        sha256: format!("{:x}", hash.finalize()),
    })
}
pub fn manifest(root: &Path) -> Result<BTreeMap<PathBuf, Fingerprint>> {
    reject_links(root)?;
    let mut files = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut bytes = 0;
    let mut entries = 0;
    while let Some((dir, depth)) = stack.pop() {
        ensure!(depth <= 32, "Ownership scan exceeds 32 directory levels");
        for item in fs::read_dir(&dir)? {
            let path = item?.path();
            entries += 1;
            ensure!(entries <= MAX_FILES, "Ownership scan exceeds 5,000 entries");
            reject_links(&path)?;
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                stack.push((path, depth + 1));
            } else if meta.is_file() {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & (0x4000 | 0x1000) == 0,
                        "Encrypted/offline files are excluded"
                    );
                }
                let fingerprint = hash_file(&path, MAX_BYTES - bytes)?;
                bytes += fingerprint.bytes;
                files.insert(path.strip_prefix(root)?.to_path_buf(), fingerprint);
            } else {
                bail!("Special files are excluded");
            }
        }
    }
    Ok(files)
}

pub fn review(
    snapshot: &Snapshot,
    inventory: &[InstalledApp],
    policy: &SafetyPolicy,
) -> Result<Option<Candidate>> {
    ensure!(
        !inventory.iter().any(|app| app.id == snapshot.app_id),
        "The uninstall registration still exists. Finish the publisher's uninstaller and refresh first"
    );
    if !snapshot.root.exists() {
        return Ok(None);
    }
    // Reapply root boundaries and overlap checks after uninstall, without relying on a removed executable.
    reject_links(&snapshot.root)?;
    let root = fs::canonicalize(&snapshot.root)?;
    ensure!(
        folded(&root) == folded(&snapshot.root),
        "Installation directory identity changed"
    );
    let parent = root.parent().context("Missing parent")?;
    ensure!(
        policy
            .allowed_parents
            .iter()
            .filter_map(|p| fs::canonicalize(p).ok())
            .any(|p| folded(&p) == folded(parent)),
        "Root is no longer allowed"
    );
    for excluded in &policy.excluded_roots {
        ensure!(!within(&root, excluded), "Protected location");
    }
    for app in inventory {
        if let Some(path) = &app.install_location {
            let path = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            ensure!(
                !within(&root, &path) && !within(&path, &root),
                "Another registration now uses this folder"
            );
        }
    }
    let files = manifest(&root)?;
    ensure!(
        files.iter().all(|(p, f)| snapshot.files.get(p) == Some(f)),
        "New or changed files were found after uninstall; this folder cannot be quarantined"
    );
    Ok(Some(Candidate {
        snapshot: snapshot.clone(),
        files,
    }))
}

pub fn quarantine(candidate: &Candidate, journal_dir: &Path) -> Result<QuarantineRecord> {
    let root = &candidate.snapshot.root;
    ensure!(
        manifest(root)? == candidate.files,
        "Preview is stale; files changed. Review again"
    );
    let id = uuid::Uuid::new_v4().to_string();
    let backup = root
        .parent()
        .context("No parent")?
        .join(format!(".AppUninstaller-quarantine-{id}"));
    let record = QuarantineRecord {
        id: id.clone(),
        app_name: candidate.snapshot.app_name.clone(),
        original: root.clone(),
        backup,
        files: candidate.files.clone(),
    };
    fs::create_dir_all(journal_dir)?;
    let path = journal_dir.join(format!("{id}.json"));
    let mut journal = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    journal.write_all(&serde_json::to_vec_pretty(&record)?)?;
    journal.sync_all()?;
    // Same-volume rename is reversible, preserves all file data/ACLs/streams, and never recursively deletes.
    fs::rename(root, &record.backup).context(
        "Quarantine rename failed; no elevation is attempted. Original data remains in place",
    )?;
    Ok(record)
}

pub fn records(dir: &Path) -> Result<Vec<QuarantineRecord>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(dir)?.take(1000) {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "json") {
            ensure!(
                fs::metadata(&path)?.len() < 4 * 1024 * 1024,
                "Quarantine journal is too large"
            );
            let record: QuarantineRecord = serde_json::from_slice(&fs::read(&path)?)?;
            validate_record(&record)?;
            if record.backup.exists() {
                records.push(record);
            }
        }
    }
    records.sort_by(|a, b| a.app_name.cmp(&b.app_name));
    Ok(records)
}
fn validate_record(record: &QuarantineRecord) -> Result<()> {
    uuid::Uuid::parse_str(&record.id)?;
    let parent = record.original.parent().context("Invalid original path")?;
    ensure!(
        record.original.is_absolute()
            && record.backup == parent.join(format!(".AppUninstaller-quarantine-{}", record.id)),
        "Invalid quarantine journal paths"
    );
    Ok(())
}
pub fn restore(record: &QuarantineRecord) -> Result<()> {
    validate_record(record)?;
    ensure!(
        !record.original.exists(),
        "Original path already exists. Restore never overwrites files"
    );
    reject_links(&record.backup)?;
    ensure!(
        manifest(&record.backup)? == record.files,
        "Quarantined data has changed; manual recovery is required"
    );
    fs::rename(&record.backup, &record.original)
        .context("Restore failed; quarantine data is preserved")?;
    Ok(())
}
