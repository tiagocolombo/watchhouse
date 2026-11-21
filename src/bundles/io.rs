use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
const PART_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 1024 * 1024;
const PART_COUNT: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    path: String,
    section: String,
    digest: String,
    bytes: u64,
    rows: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    header: Header,
    counts: Counts,
    parts: Vec<Part>,
}
struct OpenPart {
    file: File,
    part: Part,
    hash: Sha256,
}
pub(super) struct Writer {
    root: PathBuf,
    parts: Vec<Part>,
    current: Option<OpenPart>,
    bytes: u64,
}
impl Writer {
    pub(super) fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            parts: vec![],
            current: None,
            bytes: 0,
        }
    }
    fn close(&mut self) -> Result<()> {
        if let Some(mut open) = self.current.take() {
            open.file.flush().map_err(io_error)?;
            open.file.sync_all().map_err(io_error)?;
            open.part.digest = format!("sha256:{:x}", open.hash.finalize());
            self.parts.push(open.part);
        }
        Ok(())
    }
    pub(super) fn emit(&mut self, section: &str, value: &Value) -> Result<()> {
        if section.is_empty() || !section.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            return Err(invalid("untrusted section name"));
        }
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        if bytes.len() > ROW_LIMIT {
            return Err(Error::Backpressure("case export row exceeds 8MiB".into()));
        }
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("export size overflow"))?;
        if self.bytes > EXPORT_LIMIT {
            return Err(Error::Backpressure(
                "case export exceeds 16GiB; incomplete destination retained".into(),
            ));
        }
        if self.current.as_ref().is_some_and(|p| {
            p.part.section != section || p.part.bytes + bytes.len() as u64 > PART_LIMIT
        }) {
            self.close()?;
        }
        if self.current.is_none() {
            if self.parts.len() >= PART_COUNT {
                return Err(invalid("too many export parts"));
            }
            let path = format!("{section}-{:05}.jsonl", self.parts.len() + 1);
            self.current = Some(OpenPart {
                file: crate::backups::create_file(&self.root.join(&path))?,
                part: Part {
                    path,
                    section: section.into(),
                    digest: String::new(),
                    bytes: 0,
                    rows: 0,
                },
                hash: Sha256::new(),
            });
        }
        let open = self.current.as_mut().unwrap();
        open.file.write_all(&bytes).map_err(io_error)?;
        open.hash.update(&bytes);
        open.part.bytes += bytes.len() as u64;
        open.part.rows += 1;
        Ok(())
    }
    pub(super) fn finish(mut self, header: &Header, counts: &Counts) -> Result<String> {
        self.close()?;
        let manifest = Manifest {
            format: "watchhouse-case-export-v1".into(),
            header: header.clone(),
            counts: counts.clone(),
            parts: self.parts,
        };
        let raw = serde_json::to_vec_pretty(&manifest)?;
        if raw.len() as u64 > MANIFEST_LIMIT {
            return Err(invalid("manifest too large"));
        }
        let mut f = crate::backups::create_file(&self.root.join("manifest.json"))?;
        f.write_all(&raw).map_err(io_error)?;
        f.sync_all().map_err(io_error)?;
        Ok(format!("sha256:{:x}", Sha256::digest(&raw)))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    pub header: Header,
    pub counts: Counts,
    pub manifest_digest: String,
    pub files: usize,
    pub bytes: u64,
    pub verification: String,
}
fn hash(path: &Path, expected: u64) -> Result<String> {
    let meta = fs::symlink_metadata(path).map_err(io_error)?;
    if !meta.file_type().is_file() || meta.len() != expected {
        return Err(invalid("changed or linked part"));
    }
    let mut f = File::open(path).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 32768];
    loop {
        let n = f.read(&mut buf).map_err(io_error)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}
pub fn inspect(directory: &Path, expected: Option<&str>) -> Result<Inspection> {
    inspect_inner(directory, expected, false)
}
pub(super) fn inspect_inner(
    directory: &Path,
    expected: Option<&str>,
    incomplete: bool,
) -> Result<Inspection> {
    if !incomplete && directory.join("export-incomplete.json").exists() {
        return Err(invalid("incomplete export; OS reconciliation required"));
    }
    let file = crate::backups::contained(directory, "manifest.json")?;
    let meta = fs::metadata(&file).map_err(io_error)?;
    if meta.len() > MANIFEST_LIMIT {
        return Err(invalid("manifest too large"));
    }
    let bytes = fs::read(&file).map_err(io_error)?;
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    if expected.is_some_and(|value| value != digest) {
        return Err(invalid("manifest digest mismatch"));
    }
    let m: Manifest = serde_json::from_slice(&bytes)?;
    if m.format != "watchhouse-case-export-v1"
        || m.header.audience != "internal_case"
        || m.parts.len() > PART_COUNT
    {
        return Err(invalid("unsupported bundle format or bounds"));
    }
    let mut paths = std::collections::BTreeSet::from(["manifest.json".to_string()]);
    if incomplete && directory.join("export-incomplete.json").exists() {
        paths.insert("export-incomplete.json".into());
    }
    let mut counts = Counts::default();
    let mut total = 0u64;
    for (index, part) in m.parts.iter().enumerate() {
        if part.path != format!("{}-{:05}.jsonl", part.section, index + 1)
            || part.bytes > PART_LIMIT
            || part.section.is_empty()
            || !part
                .section
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_')
            || part.path.contains('/')
            || !paths.insert(part.path.clone())
        {
            return Err(invalid("invalid or duplicate part"));
        }
        total = total
            .checked_add(part.bytes)
            .ok_or_else(|| invalid("byte count overflow"))?;
        if total > EXPORT_LIMIT {
            return Err(invalid("export exceeds total bound"));
        }
        let path = crate::backups::contained(directory, &part.path)?;
        if hash(&path, part.bytes)? != part.digest {
            return Err(invalid("part digest mismatch"));
        }
        let mut input = BufReader::new(File::open(path).map_err(io_error)?);
        let mut rows = 0u64;
        loop {
            let mut line = Vec::new();
            let n = Read::by_ref(&mut input)
                .take(ROW_LIMIT as u64 + 1)
                .read_until(b'\n', &mut line)
                .map_err(io_error)?;
            if n == 0 {
                break;
            }
            if line.len() > ROW_LIMIT || line.last() != Some(&b'\n') {
                return Err(invalid("overlong or unterminated record"));
            }
            let value: Value = serde_json::from_slice(&line)?;
            validate_row(&part.section, &value, &m.header)?;
            counts.observe(&part.section, &value)?;
            rows += 1;
        }
        if rows != part.rows {
            return Err(invalid("part row count mismatch"));
        }
    }
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("non-UTF8 export path"))?;
        if !paths.contains(&name) || !entry.file_type().map_err(io_error)?.is_file() {
            return Err(invalid("undeclared or linked export entry"));
        }
    }
    if counts != m.counts || counts.sections.get("cases") != Some(&1) {
        return Err(invalid("manifest counts or case record mismatch"));
    }
    Ok(Inspection{header:m.header,counts,manifest_digest:digest,files:paths.len(),bytes:total,verification:"Internal case record-view and included-byte integrity verified; original raw columns with omissions are hash references only. Missing/corrupt/redacted evidence remains declared. No live source, model, native execution, restoration or recovery is verified.".into()})
}
