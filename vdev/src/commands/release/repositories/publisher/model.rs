//! Stable on-disk and S3 formats shared by publication, retries and rollback.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(super) const LIMIT: u64 = 5 * 1024 * 1024 * 1024;

pub(super) fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(value)?;
    value.sort_all_objects();
    let text = serde_json::to_string(&value)?;
    // Preserve the original manifest encoding, including Python's ensure_ascii.
    let mut bytes = Vec::new();
    for character in text.chars() {
        if character.is_ascii() {
            bytes.push(character as u8);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                write!(bytes, "\\u{unit:04x}")?;
            }
        }
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub(super) fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub sha256: String,
    pub size: u64,
}
impl Entry {
    pub fn bytes(bytes: &[u8]) -> Self {
        Self {
            sha256: digest(bytes),
            size: bytes.len() as u64,
        }
    }
    pub fn file(path: &Path) -> Result<Self> {
        ensure!(
            path.symlink_metadata()?.is_file(),
            "Expected a regular file: {}",
            path.display()
        );
        Self::read(fs::File::open(path)?)
    }
    pub fn read(mut source: impl Read) -> Result<Self> {
        let mut hash = Sha256::new();
        let mut size = 0;
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size += count as u64;
            hash.update(&buffer[..count]);
        }
        Ok(Self {
            sha256: hex::encode(hash.finalize()),
            size,
        })
    }
}

pub(super) fn safe_relative(name: &str) -> Result<&Path> {
    let parts: Vec<_> = name.split('/').collect();
    ensure!(
        name.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./+~-".contains(&b))
            && parts.len() >= 3
            && matches!(parts[0], "apt" | "yum")
            && parts
                .iter()
                .all(|p| !p.is_empty() && *p != "." && *p != ".."),
        "Invalid repository path: {name}"
    );
    Ok(Path::new(name))
}
pub(super) fn mutable(name: &str) -> bool {
    matches!(
        name.rsplit('/').next(),
        Some(
            "Packages"
                | "Packages.gz"
                | "Release"
                | "Release.gpg"
                | "InRelease"
                | "repomd.xml"
                | "repomd.xml.asc"
        )
    )
}
pub(super) fn root_order(name: &str) -> u8 {
    match name.rsplit('/').next().unwrap_or("") {
        "Release" => 0,
        "Release.gpg" => 1,
        "InRelease" => 2,
        "repomd.xml" => 3,
        "repomd.xml.asc" => 4,
        _ => 5,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    base: Option<String>,
    input: Option<String>,
    files: BTreeMap<String, Entry>,
}
#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub entries: BTreeMap<String, Entry>,
    pub base: Option<String>,
    pub input: Option<String>,
    pub data: Vec<u8>,
    pub id: String,
    pub source: PathBuf,
}
impl Snapshot {
    pub fn new(
        entries: BTreeMap<String, Entry>,
        base: Option<String>,
        input: Option<String>,
        source: PathBuf,
    ) -> Result<Self> {
        ensure!(
            !entries.is_empty() && entries.keys().any(|n| mutable(n)),
            "Snapshot has no repository metadata"
        );
        for (name, entry) in &entries {
            safe_relative(name)?;
            ensure!(
                is_digest(&entry.sha256) && entry.size <= LIMIT,
                "Invalid snapshot entry"
            );
        }
        for id in [&base, &input].into_iter().flatten() {
            ensure!(is_digest(id), "Invalid snapshot identity");
        }
        let data = canonical(&Manifest {
            schema: 2,
            base: base.clone(),
            input: input.clone(),
            files: entries.clone(),
        })?;
        Ok(Self {
            entries,
            base,
            input,
            id: digest(&data),
            data,
            source,
        })
    }
    pub fn decode(data: &[u8], expected: Option<&str>) -> Result<Self> {
        let value: Manifest = serde_json::from_slice(data)?;
        ensure!(value.schema == 2, "Unsupported snapshot schema");
        let snapshot = Self::new(value.files, value.base, value.input, PathBuf::new())?;
        ensure!(
            snapshot.data == data && expected.is_none_or(|id| id == snapshot.id),
            "Noncanonical manifest or incorrect digest"
        );
        Ok(snapshot)
    }
    pub fn local(source: &Path) -> Result<Self> {
        let mut snapshot = Self::decode(
            &fs::read(
                source
                    .parent()
                    .context("Missing snapshot parent")?
                    .join("manifest.json"),
            )?,
            None,
        )?;
        let files = files(source)?;
        for (name, path) in &files {
            ensure!(
                snapshot.entries.get(name) == Some(&Entry::file(path)?),
                "Snapshot content differs: {name}"
            );
        }
        for name in snapshot.entries.keys() {
            ensure!(
                files.contains_key(name) || (!mutable(name) && snapshot.base.is_some()),
                "Missing snapshot file: {name}"
            );
        }
        source.clone_into(&mut snapshot.source);
        Ok(snapshot)
    }
}

/// Walk without following symlinks, including at the root.
pub(super) fn files(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<String, PathBuf>) -> Result<()> {
        ensure!(
            directory.symlink_metadata()?.is_dir(),
            "Expected a regular directory: {}",
            directory.display()
        );
        for item in fs::read_dir(directory)? {
            let path = item?.path();
            let kind = path.symlink_metadata()?.file_type();
            if kind.is_dir() {
                visit(root, &path, result)?;
            } else {
                ensure!(kind.is_file(), "Nonregular file: {}", path.display());
                result.insert(
                    path.strip_prefix(root)?
                        .to_str()
                        .context("Non-UTF8 path")?
                        .replace('\\', "/"),
                    path,
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}
pub(super) fn write(path: &Path, data: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().context("Missing parent")?)?;
    ensure!(!path.is_symlink(), "Refusing a symlink output");
    fs::write(path, data)?;
    Ok(())
}
pub(super) fn copy(source: &Path, destination: &Path) -> Result<()> {
    ensure!(
        source.symlink_metadata()?.is_file() && !destination.is_symlink(),
        "Copy requires regular files"
    );
    fs::create_dir_all(destination.parent().context("Missing parent")?)?;
    fs::copy(source, destination)?;
    Ok(())
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub schema: u32,
    #[serde(default = "default_true")]
    pub require_existing_repository: bool,
    pub repository: String,
    pub repository_id: u64,
    pub workflow: String,
    pub account: String,
    pub bucket: String,
    pub region: String,
    pub live_prefix: String,
    pub archive_prefix: String,
    pub bundle_prefix: String,
    pub control_key: String,
    pub public_key: String,
    pub primary_fingerprint: String,
    pub signing_fingerprint: String,
    pub apt_url: String,
    pub yum_url: String,
    #[serde(skip)]
    pub key_path: PathBuf,
}
fn default_true() -> bool {
    true
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut config: Self = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            config.schema == 1
                && config.repository == "vectordotdev/vector"
                && config.repository_id == 146_327_667,
            "Wrong publisher configuration/source repository"
        );
        ensure!(
            config.account.len() == 12 && config.account.bytes().all(|b| b.is_ascii_digit()),
            "Invalid AWS account"
        );
        ensure!(
            regex::Regex::new(r"^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$")?.is_match(&config.bucket),
            "Invalid bucket"
        );
        let paths = [
            &config.live_prefix,
            &config.archive_prefix,
            &config.bundle_prefix,
            &config.control_key,
        ];
        for (index, name) in paths.iter().enumerate() {
            ensure!(
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
                    && name
                        .trim_end_matches('/')
                        .split('/')
                        .all(|p| !p.is_empty() && p != "." && p != ".."),
                "Invalid S3 path"
            );
            ensure!(index == 3 || name.ends_with('/'), "Prefixes must end in /");
            for other in &paths[index + 1..] {
                ensure!(
                    !name.starts_with(other.as_str()) && !other.starts_with(name.as_str()),
                    "S3 paths must be disjoint"
                );
            }
        }
        if config.account == "486234852809" {
            ensure!(
                config.bucket == "dd-cose-releases" && config.live_prefix == "vector/repositories/",
                "Unexpected production destination"
            );
            ensure!(
                paths[1..].iter().all(|p| !p.starts_with("vector/")),
                "Internal state cannot use the public prefix"
            );
        }
        for fingerprint in [&config.primary_fingerprint, &config.signing_fingerprint] {
            ensure!(
                fingerprint.len() == 40
                    && fingerprint
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)),
                "Invalid pinned fingerprint"
            );
        }
        ensure!(
            !config.public_key.is_empty()
                && Path::new(&config.public_key)
                    .file_name()
                    .is_some_and(|p| p == config.public_key.as_str()),
            "Public key must be beside configuration"
        );
        config.key_path = path
            .parent()
            .context("Missing configuration parent")?
            .join(&config.public_key);
        Ok(config)
    }
}
pub(super) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Missing/invalid {key}"))
}
pub(super) fn entry_value(value: &Value) -> Result<Entry> {
    Ok(serde_json::from_value(
        json!({"size":value["size"], "sha256":value["sha256"]}),
    )?)
}

/// Publishing has its own extraction budgets and does not depend on Vector's
/// runtime crates. Cap the decompressed stream before tar/XML parsing.
#[allow(clippy::disallowed_types)]
pub(super) fn gzip(source: impl Read, limit: u64) -> impl Read {
    flate2::read::GzDecoder::new(source).take(limit)
}
