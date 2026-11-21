//! OS-owned consistent exports. Inspection never opens an executor or imports authority.
use crate::{
    application::{Application, text},
    error::{Error, Result},
    real_execution::{pipeline::Candidate, protocol::InputFile},
    repair, store,
};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const DATABASE_LIMIT: u64 = 16 * 1024 * 1024 * 1024;
const ARTIFACT_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const FILE_LIMIT: u64 = 8 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 16 * 1024 * 1024;
const ENTRY_LIMIT: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub kind: String,
    pub identity: String,
    pub directory: String,
    pub files: Vec<InputFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub id: String,
    pub created_at: String,
    pub operator: String,
    pub reason: String,
    pub migrations: Vec<(i64, String)>,
    pub entries: Vec<Entry>,
    pub artifacts: Vec<Artifact>,
    pub excluded: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub id: String,
    pub manifest_digest: String,
    pub created_at: String,
    pub files: usize,
    pub bytes: u64,
    pub artifacts: usize,
    pub verification: String,
    pub restore_available: bool,
}
fn io(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("backup I/O: {e}"))
}
fn invalid(s: &str) -> Error {
    Error::Conflict(format!("backup integrity: {s}"))
}
fn regular(path: &Path, max: u64) -> Result<File> {
    let m = fs::symlink_metadata(path).map_err(io)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > max {
        return Err(invalid("missing, linked, special or oversized file"));
    }
    File::open(path).map_err(io)
}
fn checksum(path: &Path, max: u64) -> Result<(u64, String)> {
    let mut f = regular(path, max)?;
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b).map_err(io)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            return Err(invalid("file grew beyond bound"));
        }
        hash.update(&b[..n]);
    }
    Ok((total, format!("sha256:{:x}", hash.finalize())))
}
pub(crate) fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut b = fs::DirBuilder::new();
        b.mode(0o700);
        b.create(path).map_err(io)
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(io)
}
pub(crate) fn create_file(path: &Path) -> Result<File> {
    let mut b = OpenOptions::new();
    b.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        b.mode(0o600);
    }
    b.open(path).map_err(io)
}
fn portable(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && path.split('/').all(|p| {
            let stem = p.split('.').next().unwrap_or("").to_ascii_uppercase();
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.len() <= 128
                && !p.ends_with('.')
                && !p.eq_ignore_ascii_case(".git")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit())
        })
}
// Reject linked directories as well as linked final files. Host-administrator replacement
// during an OS-owned export remains outside the single-operator trust boundary.
pub(crate) fn contained(root: &Path, relative: &str) -> Result<PathBuf> {
    if !portable(relative) || !fs::symlink_metadata(root).map_err(io)?.is_dir() {
        return Err(invalid("non-portable path or invalid root"));
    }
    if fs::symlink_metadata(root)
        .map_err(io)?
        .file_type()
        .is_symlink()
    {
        return Err(invalid("linked root"));
    }
    let mut p = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        p.push(part);
        let m = fs::symlink_metadata(&p).map_err(io)?;
        if m.file_type().is_symlink() || (i + 1 < parts.len() && !m.is_dir()) {
            return Err(invalid("linked or non-directory path component"));
        }
    }
    Ok(p)
}
fn migrations(db: &Connection) -> Result<Vec<(i64, String)>> {
    let rows = db
        .prepare("SELECT version,digest FROM schema_migrations ORDER BY version")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() < 31 || rows.len() > store::MIGRATIONS.len() {
        return Err(invalid("unsupported migration history"));
    }
    for ((version, sum), (expected, sql)) in rows.iter().zip(store::MIGRATIONS) {
        if version != expected
            || (sum != &store::migration_digest(sql)
                && sum != &store::digest(&sql.replace("\r\n", "\n").replace('\n', "\r\n")))
        {
            return Err(invalid("migration history changed"));
        }
    }
    Ok(rows)
}
fn database(path: &Path) -> Result<Connection> {
    regular(path, DATABASE_LIMIT)?;
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.pragma_update(None, "trusted_schema", "OFF")?;
    let check: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if check != "ok"
        || db
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(invalid("database integrity/foreign keys"));
    }
    migrations(&db)?;
    let mut s = db.prepare("SELECT digest,content FROM evidence")?;
    let mut rows = s.query([])?;
    while let Some(r) = rows.next()? {
        let (digest, content): (String, String) = (r.get(0)?, r.get(1)?);
        if store::digest(&content) != digest {
            return Err(invalid("evidence content changed"));
        }
    }
    drop(rows);
    drop(s);
    Ok(db)
}
fn artifacts(db: &Connection) -> Result<Vec<(Artifact, PathBuf)>> {
    let mut result = vec![];
    let mut stmt = db.prepare("SELECT revision FROM repair_sources ORDER BY revision")?;
    for revision in stmt.query_map([], |r| r.get::<_, i64>(0))? {
        if result.len() >= 10_000 {
            return Err(invalid("too many artifact roots"));
        }
        let revision = revision?;
        let s = repair::source(db, revision)?;
        if s.config.revision != revision {
            return Err(invalid("source revision mismatch"));
        }
        let a = Artifact {
            kind: "source".into(),
            identity: s.config.revision.to_string(),
            directory: format!("artifacts/source-{}", s.config.revision),
            files: s
                .snapshot
                .files
                .into_iter()
                .map(|f| InputFile {
                    path: f.path,
                    bytes: f.bytes,
                    digest: f.digest,
                    executable: f.git_mode == "100755",
                })
                .collect(),
        };
        result.push((a, s.snapshot_path));
    }
    let mut stmt = db.prepare("SELECT id,record FROM repair_candidates ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        if result.len() >= 10_000 {
            return Err(invalid("too many artifact roots"));
        }
        let (id, raw) = row?;
        let c: Candidate = serde_json::from_str(&raw)?;
        if c.id != id
            || !portable(&id)
            || id.contains('/')
            || c.digest != store::digest(&serde_json::to_string(&c.files)?)
        {
            return Err(invalid("candidate identity changed"));
        }
        let a = Artifact {
            kind: "candidate".into(),
            identity: id.clone(),
            directory: format!("artifacts/{id}"),
            files: c.files,
        };
        let root = crate::restore::materialization(db, "candidate", &id)?.unwrap_or(c.root);
        result.push((a, root));
    }
    if result.len() > 10_000 {
        return Err(invalid("too many artifact roots"));
    }
    Ok(result)
}
fn inventory(root: &Path, relative: &str, entries: &mut BTreeSet<String>) -> Result<()> {
    if entries.len() > ENTRY_LIMIT {
        return Err(invalid("too many directory entries"));
    }
    let p = if relative.is_empty() {
        root.to_path_buf()
    } else {
        contained(root, relative)?
    };
    for item in fs::read_dir(p).map_err(io)? {
        if entries.len() >= ENTRY_LIMIT {
            return Err(invalid("too many directory entries"));
        }
        let item = item.map_err(io)?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| invalid("non-Unicode name"))?;
        let name = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        let path = contained(root, &name)?;
        let m = fs::symlink_metadata(&path).map_err(io)?;
        if m.is_dir() {
            if !entries.insert(format!("{name}/")) {
                return Err(invalid("duplicate directory"));
            }
            inventory(root, &name, entries)?;
        } else if m.is_file() {
            entries.insert(name);
        } else {
            return Err(invalid("special file"));
        }
    }
    Ok(())
}
fn expected_inventory(m: &Manifest) -> Result<BTreeSet<String>> {
    let mut expected = BTreeSet::from(["manifest.json".into()]);
    let mut folded = HashSet::new();
    let mut components = HashMap::new();
    for e in &m.entries {
        if !portable(&e.path)
            || e.path == "manifest.json"
            || !folded.insert(e.path.to_ascii_lowercase())
            || !expected.insert(e.path.clone())
        {
            return Err(invalid("duplicate or unsafe manifest entry"));
        }
        let mut at = e.path.as_str();
        loop {
            let old = components.insert(at.to_ascii_lowercase(), at.to_owned());
            if old.is_some_and(|old| old != at) {
                return Err(invalid("case-colliding path components"));
            }
            let Some((parent, _)) = at.rsplit_once('/') else {
                break;
            };
            at = parent;
        }
        let mut at = e.path.as_str();
        while let Some((parent, _)) = at.rsplit_once('/') {
            expected.insert(format!("{parent}/"));
            at = parent;
        }
    }
    Ok(expected)
}
pub(crate) fn manifest_text(directory: &Path) -> Result<String> {
    let mut raw = String::new();
    regular(&directory.join("manifest.json"), MANIFEST_LIMIT)?
        .take(MANIFEST_LIMIT + 1)
        .read_to_string(&mut raw)
        .map_err(io)?;
    if raw.len() as u64 > MANIFEST_LIMIT {
        return Err(invalid("manifest grew beyond bound"));
    }
    Ok(raw)
}
/// Read-only verification, optionally pinned to an independently retained manifest hash.
pub fn inspect(directory: &Path, expected_digest: Option<&str>) -> Result<Inspection> {
    let root_meta = fs::symlink_metadata(directory).map_err(io)?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err(invalid("invalid backup root"));
    }
    let raw = manifest_text(directory)?;
    let digest = store::digest(&raw);
    if expected_digest.is_some_and(|d| d != digest) {
        return Err(invalid("manifest digest mismatch"));
    }
    let m: Manifest = serde_json::from_str(&raw)?;
    if m.version != 1 || m.entries.len() > ENTRY_LIMIT || m.artifacts.len() > 10_000 {
        return Err(invalid("unsupported manifest/bounds"));
    }
    let mut observed = BTreeSet::new();
    inventory(directory, "", &mut observed)?;
    if observed != expected_inventory(&m)? {
        return Err(invalid("extra or missing backup content"));
    }
    let mut total = 0u64;
    let mut ordinary = 0u64;
    let mut db_count = 0;
    for e in &m.entries {
        let max = if e.path == "watchhouse.db" {
            db_count += 1;
            DATABASE_LIMIT
        } else {
            FILE_LIMIT
        };
        if checksum(&contained(directory, &e.path)?, max)? != (e.bytes, e.digest.clone()) {
            return Err(invalid("file digest/size mismatch"));
        }
        total = total
            .checked_add(e.bytes)
            .ok_or_else(|| invalid("byte counter overflow"))?;
        if e.path != "watchhouse.db" {
            ordinary += e.bytes;
        }
    }
    if db_count != 1 || ordinary > ARTIFACT_LIMIT {
        return Err(invalid("database count or artifact bound"));
    }
    let db = database(&directory.join("watchhouse.db"))?;
    if migrations(&db)? != m.migrations {
        return Err(invalid("manifest/database migration mismatch"));
    }
    let declared: Vec<_> = artifacts(&db)?.into_iter().map(|(a, _)| a).collect();
    if declared != m.artifacts {
        return Err(invalid("manifest/database artifact mismatch"));
    }
    let wanted: BTreeSet<_> = m
        .artifacts
        .iter()
        .flat_map(|a| {
            a.files
                .iter()
                .map(|f| format!("{}/{}", a.directory, f.path))
        })
        .chain(std::iter::once("watchhouse.db".into()))
        .collect();
    if wanted != m.entries.iter().map(|e| e.path.clone()).collect() {
        return Err(invalid("undeclared or missing frozen artifact"));
    }
    for a in &m.artifacts {
        let mut names = HashSet::new();
        for f in &a.files {
            if !portable(&f.path)
                || !names.insert(f.path.to_ascii_lowercase())
                || !m.entries.iter().any(|e| {
                    e.path == format!("{}/{}", a.directory, f.path)
                        && e.digest == f.digest
                        && e.bytes == f.bytes
                })
            {
                return Err(invalid("frozen artifact identity mismatch"));
            }
        }
    }
    Ok(Inspection { id:m.id, manifest_digest:digest, created_at:m.created_at, files:m.entries.len(), bytes:total, artifacts:m.artifacts.len(), verification:"consistent SQLite snapshot, migration history, foreign keys, evidence and complete declared frozen artifacts verified; no live integrations exercised".into(), restore_available:true })
}
impl Application {
    /// No HTTP endpoint accepts a filesystem destination or returns private backup bytes.
    pub fn create_backup(
        &self,
        directory: &Path,
        operator: &str,
        reason: &str,
    ) -> Result<Inspection> {
        text(operator, "operator", 128)?;
        text(reason, "reason", 2000)?;
        if operator.starts_with("fake-") || operator.starts_with("model-") {
            return Err(Error::Invalid("OS operator required".into()));
        }
        private_directory(directory)?;
        create_file(&directory.join("backup-incomplete"))?
            .sync_all()
            .map_err(io)?;
        // VACUUM INTO sees one SQLite read snapshot, including committed live WAL pages.
        // The manifest is written last; interrupted directories are visibly incomplete.
        let db_path = directory.join("watchhouse.db");
        let destination = db_path
            .to_str()
            .ok_or_else(|| invalid("non-Unicode destination"))?;
        self.store
            .connect()?
            .execute("VACUUM INTO ?1", [destination])?;
        let db = database(&db_path)?;
        let mut entries = vec![];
        let (bytes, digest) = checksum(&db_path, DATABASE_LIMIT)?;
        entries.push(Entry {
            path: "watchhouse.db".into(),
            bytes,
            digest,
        });
        let mut total = 0u64;
        let mut frozen = vec![];
        for (a, root) in artifacts(&db)? {
            for f in &a.files {
                if entries.len() >= ENTRY_LIMIT {
                    return Err(invalid("too many artifact files"));
                }
                let input = contained(&root, &f.path)?;
                let name = format!("{}/{}", a.directory, f.path);
                if !portable(&name) {
                    return Err(invalid("unsafe artifact path"));
                }
                let target = directory.join(&name);
                fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
                let mut src = regular(&input, FILE_LIMIT)?;
                let mut dst = create_file(&target)?;
                let copied =
                    std::io::copy(&mut Read::by_ref(&mut src).take(FILE_LIMIT + 1), &mut dst)
                        .map_err(io)?;
                dst.sync_all().map_err(io)?;
                if copied > FILE_LIMIT
                    || checksum(&target, FILE_LIMIT)? != (f.bytes, f.digest.clone())
                {
                    return Err(invalid("frozen input changed or exceeds bound"));
                }
                total += copied;
                if total > ARTIFACT_LIMIT {
                    return Err(invalid("artifact bytes exceed backup bound"));
                }
                entries.push(Entry {
                    path: name,
                    bytes: f.bytes,
                    digest: f.digest.clone(),
                });
            }
            frozen.push(a);
        }
        let m = Manifest {version:1,id:store::id("backup"),created_at:self.now(),operator:operator.into(),reason:reason.into(),migrations:migrations(&db)?,entries,artifacts:frozen,excluded:vec!["Runtime executables, images, credential values/auth homes, worker control roots, private transcripts and live external systems; configuration/secret references are retained in the database".into()]};
        drop(db);
        // Manifest size is bounded before making the completed marker visible.
        let raw = serde_json::to_string_pretty(&m)?;
        if raw.len() as u64 > MANIFEST_LIMIT {
            return Err(invalid("manifest exceeds bound"));
        }
        let mut file = create_file(&directory.join("manifest.json"))?;
        file.write_all(raw.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::remove_file(directory.join("backup-incomplete")).map_err(io)?;
        let checked = inspect(directory, Some(&store::digest(&raw)))?;
        let record = serde_json::json!({"type":"backup_created","backup_id":checked.id,"manifest_digest":checked.manifest_digest,"at":self.now(),"operator":operator,"reason":reason,"files":checked.files,"bytes":checked.bytes,"artifacts":checked.artifacts});
        self.store.connect()?.execute(
            "INSERT INTO factory_events(record) VALUES (?1)",
            params![serde_json::to_string(&record)?],
        )?;
        Ok(checked)
    }
}
