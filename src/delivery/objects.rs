//! Git object encoding over verified bytes; never execute candidate code or Git hooks.
use crate::{
    error::{Error, Result},
    real_execution::pipeline::Candidate,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{collections::BTreeMap, fs, io::Read};
pub fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn object(kind: &str, bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("{kind} {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    format!("{:x}", h.finalize())
}
#[derive(Default)]
struct Tree {
    files: BTreeMap<String, (String, bool)>,
    dirs: BTreeMap<String, Tree>,
}
impl Tree {
    fn insert(&mut self, path: &str, sha: String, executable: bool) -> Result<()> {
        if let Some((dir, rest)) = path.split_once('/') {
            if self.files.contains_key(dir) {
                return Err(Error::Invalid("file/directory collision".into()));
            }
            self.dirs
                .entry(dir.into())
                .or_default()
                .insert(rest, sha, executable)
        } else {
            if self.dirs.contains_key(path)
                || self.files.insert(path.into(), (sha, executable)).is_some()
            {
                return Err(Error::Invalid("duplicate Git entry".into()));
            }
            Ok(())
        }
    }
    fn sha(&self) -> String {
        let mut entries: Vec<_> = self
            .files
            .iter()
            .map(|(name, (sha, executable))| {
                (
                    name.as_bytes().to_vec(),
                    format!("{} {name}", if *executable { "100755" } else { "100644" }),
                    sha.clone(),
                )
            })
            .collect();
        entries.extend(self.dirs.iter().map(|(name, dir)| {
            let mut sort = name.as_bytes().to_vec();
            sort.push(b'/');
            (sort, format!("40000 {name}"), dir.sha())
        }));
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut raw = vec![];
        for (_, entry, sha) in entries {
            raw.extend_from_slice(entry.as_bytes());
            raw.push(0);
            for pair in sha.as_bytes().chunks_exact(2) {
                raw.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
            }
        }
        object("tree", &raw)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedTree {
    pub sha: String,
    pub request: Value,
}
/// A complete local manifest determines the expected remote tree. The API payload only changes
/// selected text entries over its frozen base; a publisher must reject a different returned SHA.
pub fn prepare_tree(c: &Candidate) -> Result<PreparedTree> {
    if !oid(&c.base_commit)
        || !oid(&c.base_tree)
        || c.files.len() > 5000
        || c.changes.is_empty()
        || c.changes.len() > 20
        || serde_json::to_vec(&c.changes)?.len() > 1024 * 1024
    {
        return Err(Error::Invalid("invalid bounded publication input".into()));
    }
    if fs::canonicalize(&c.root).ok().as_ref() != Some(&c.root) {
        return Err(Error::Conflict("candidate root changed".into()));
    }
    let mut t = Tree::default();
    let mut total = 0u64;
    let mut paths = std::collections::HashSet::new();
    for f in &c.files {
        if !crate::real_execution::protocol::portable(&f.path)
            || !paths.insert(f.path.to_ascii_lowercase())
            || f.bytes > 8 * 1024 * 1024
        {
            return Err(Error::Invalid(
                "unsafe or colliding publication path".into(),
            ));
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| Error::Invalid("publication size overflow".into()))?;
        if total > 128 * 1024 * 1024 {
            return Err(Error::Invalid("publication input exceeds bound".into()));
        }
        let path = c.root.join(&f.path);
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| Error::Invalid("candidate file missing".into()))?;
        if !meta.is_file()
            || meta.len() != f.bytes
            || fs::canonicalize(&path).ok().as_ref() != Some(&path)
        {
            return Err(Error::Conflict(
                "candidate path, type or size changed".into(),
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|_| Error::Invalid("candidate file unavailable".into()))?
            .take(f.bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Invalid("candidate file read failed".into()))?;
        if bytes.len() as u64 != f.bytes {
            return Err(Error::Conflict(
                "candidate file size changed during read".into(),
            ));
        }
        if crate::real_execution::protocol::bytes_digest(&bytes) != f.digest {
            return Err(Error::Conflict("candidate content changed".into()));
        }
        t.insert(&f.path, object("blob", &bytes), f.executable)?;
    }
    let mut seen = std::collections::HashSet::new();
    let mut entries = vec![];
    for ch in &c.changes {
        if !crate::real_execution::protocol::portable(&ch.path)
            || !seen.insert(ch.path.to_ascii_lowercase())
        {
            return Err(Error::Invalid(
                "unsafe or duplicate publication change".into(),
            ));
        }
        let file = c.files.iter().find(|f| f.path == ch.path);
        let mode = if file.is_some_and(|f| f.executable) {
            "100755"
        } else {
            "100644"
        };
        if let Some(content) = &ch.after {
            if file.is_none_or(|f| f.digest != crate::store::digest(content)) {
                return Err(Error::Conflict(
                    "change differs from frozen manifest".into(),
                ));
            }
            entries.push(json!({"path":ch.path,"mode":mode,"type":"blob","content":content}));
        } else {
            if file.is_some() {
                return Err(Error::Conflict("deleted file remains in manifest".into()));
            }
            entries.push(json!({"path":ch.path,"mode":mode,"type":"blob","sha":null}));
        }
    }
    Ok(PreparedTree {
        sha: t.sha(),
        request: json!({"base_tree":c.base_tree,"tree":entries}),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_empty_tree_and_known_blob_identities() {
        assert_eq!(
            Tree::default().sha(),
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
        );
        assert_eq!(
            object("blob", b"test content\n"),
            "d670460b4b4aece5915caf5c68d12f560a9fe3e4"
        );
        let mut tree = Tree::default();
        tree.insert("a/x", object("blob", b"x"), false).unwrap();
        assert!(tree.insert("a", object("blob", b"y"), false).is_err());
        assert!(tree.insert("a/x", object("blob", b"x"), true).is_err());
    }
}

#[cfg(all(test, unix))]
mod filesystem_tests {
    use super::*;
    use crate::{
        contracts::V1,
        real_execution::{
            pipeline::{Candidate, TextChange},
            protocol::InputFile,
        },
    };
    fn fixture() -> (tempfile::TempDir, Candidate) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("a")).unwrap();
        let mut files = vec![];
        for (path, bytes) in [
            ("answer.txt", b"4\n".as_slice()),
            ("a/z.txt", b"nested".as_slice()),
            ("a.c", b"binary\0data".as_slice()),
        ] {
            fs::write(root.path().join(path), bytes).unwrap();
            files.push(InputFile {
                path: path.into(),
                bytes: bytes.len() as u64,
                digest: crate::real_execution::protocol::bytes_digest(bytes),
                executable: false,
            });
        }
        let c = Candidate {
            schema_version: V1,
            id: "candidate-fixture".into(),
            case_id: "case-fixture".into(),
            repair_id: "repair-fixture".into(),
            author: "operator".into(),
            producer: "operator-provided-text-v1".into(),
            model_job: None,
            recorded_at: "2026-11-16T12:00:00Z".into(),
            recorded_version: 1,
            source_revision: 1,
            source_digest: None,
            policy_revision: 1,
            repository: "fixture/calculator".into(),
            base_commit: "a".repeat(40),
            base_tree: "b".repeat(40),
            digest: crate::store::digest(&serde_json::to_string(&files).unwrap()),
            files,
            root: root.path().canonicalize().unwrap(),
            changes: vec![TextChange {
                path: "answer.txt".into(),
                before_digest: Some(crate::store::digest("5\n")),
                after: Some("4\n".into()),
            }],
            deadline: "2099-01-01T00:00:00Z".into(),
        };
        (root, c)
    }
    #[test]
    fn tree_identity_matches_git_including_nested_order_and_binary_base_files() {
        let (_root, c) = fixture();
        fn git(c: &Candidate, args: &[&str]) -> String {
            let o = std::process::Command::new("git")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args([
                    "-c",
                    "core.autocrlf=false",
                    "-c",
                    "core.hooksPath=",
                    "-c",
                    "core.fsmonitor=false",
                    "-C",
                ])
                .arg(&c.root)
                .args(args)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8(o.stdout).unwrap().trim().into()
        }
        git(&c, &["init", "-q"]);
        git(&c, &["add", "."]);
        assert_eq!(prepare_tree(&c).unwrap().sha, git(&c, &["write-tree"]));
        fs::write(c.root.join("answer.txt"), "6\n").unwrap();
        assert!(prepare_tree(&c).is_err());
    }
    #[test]
    fn parent_symlink_and_payload_drift_cannot_change_publication_material() {
        let (_root, mut c) = fixture();
        let foreign = tempfile::tempdir().unwrap();
        fs::write(foreign.path().join("z.txt"), b"nested").unwrap();
        fs::remove_dir_all(c.root.join("a")).unwrap();
        std::os::unix::fs::symlink(foreign.path(), c.root.join("a")).unwrap();
        assert!(prepare_tree(&c).is_err());
        fs::remove_file(c.root.join("a")).unwrap();
        fs::create_dir(c.root.join("a")).unwrap();
        fs::write(c.root.join("a/z.txt"), b"nested").unwrap();
        c.changes[0].after = Some("6\n".into());
        assert!(prepare_tree(&c).is_err());
    }
}
