//! Restore a historical selection with fresh, monotonically dated apt roots.
use super::{
    bundles::{checked_download, unpack_bundle},
    engine::{download_metadata, save_bundle},
    model::{Config, Entry, Snapshot, canonical, copy, digest, files, is_digest, mutable, write},
    repository::{
        PublicKey, Signer, dated_release, next_release_date, release_date, verify_metadata,
    },
    storage::{Publisher, S3Store, Store},
};
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    schema: u32,
    operation: String,
    pub target: String,
    pub active: String,
    pub pending: Option<String>,
}
impl Plan {
    pub fn decode(data: &[u8]) -> Result<(Self, String)> {
        let plan: Self = serde_json::from_slice(data)?;
        ensure!(
            plan.schema == 1 && plan.operation == "rollback" && canonical(&plan)? == data,
            "Invalid rollback request"
        );
        ensure!(
            is_digest(&plan.target)
                && is_digest(&plan.active)
                && plan.pending.as_ref().is_none_or(|s| is_digest(s)),
            "Invalid rollback snapshot identity"
        );
        ensure!(
            plan.pending.is_none() || plan.target == plan.active,
            "Abandoning pending publication must restore last completed selection"
        );
        Ok((plan, digest(data)))
    }
    fn source_ids(&self) -> BTreeSet<&str> {
        [
            Some(self.target.as_str()),
            Some(self.active.as_str()),
            self.pending.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
fn request_key(config: &Config, input: &str) -> String {
    format!("{}rollback-requests/{input}.json", config.bundle_prefix)
}
fn bundle_key(config: &Config, input: &str) -> String {
    format!("{}rollbacks/{input}.tar.gz", config.bundle_prefix)
}
pub(super) fn base(config: &Config, work: &Path, target: &str) -> Result<()> {
    let store = S3Store::new(config)?;
    let publisher = Publisher::new(&store, config);
    let (state, _) = publisher.load_state()?;
    ensure!(!state.held, "Rollback requires unlocked repository");
    let active = state
        .active
        .context("Rollback requires initialized repository")?;
    let candidate = publisher.archived(state.pending.as_deref().unwrap_or(&active))?;
    let mut saved = None;
    if let Some(input) = &candidate.input {
        let key = request_key(config, input);
        if store.head(&key)?.is_some() {
            let (data, _) = store.read(&key)?;
            let (plan, identity) = Plan::decode(&data)?;
            ensure!(
                identity == *input,
                "Saved rollback request checksum mismatch"
            );
            if plan.target == target {
                saved = Some(data);
            }
        }
    }
    let retry = saved.is_some();
    let data = saved.unwrap_or(canonical(&Plan {
        schema: 1,
        operation: "rollback".into(),
        target: target.into(),
        active,
        pending: state.pending,
    })?);
    let (plan, input) = Plan::decode(&data)?;
    write(&work.join("input.json"), &data)?;
    for identity in plan.source_ids() {
        let snapshot = publisher.archived(identity)?;
        let directory = work.join("sources").join(identity);
        ensure!(!directory.exists(), "Rollback source already exists");
        write(&directory.join("manifest.json"), &snapshot.data)?;
        download_metadata(
            &publisher,
            &snapshot,
            &directory.join("files"),
            identity == plan.active || identity == plan.target,
        )?;
    }
    let key = bundle_key(config, &input);
    if store.head(&key)?.is_some() {
        let archive = work.join("cached.tar.gz");
        checked_download(&store, &key, &archive, None)?;
        let snapshot = unpack_bundle(&archive, &work.join("result"))?;
        ensure!(
            snapshot.input.as_deref() == Some(&input)
                && snapshot.base.as_deref() == Some(&plan.active)
                && fs::read(work.join("result/input.json"))? == data,
            "Saved rollback belongs to another request"
        );
        ensure!(
            !retry || snapshot.id == candidate.id,
            "Saved rollback differs from pending/completed generation"
        );
        fs::remove_file(archive)?;
        println!("Restored saved signed rollback; signing skipped");
    } else {
        ensure!(
            candidate.input.as_deref() != Some(&input),
            "Required rollback retry bundle missing"
        );
        println!("Downloaded archived selections and potentially exposed dates");
    }
    Ok(())
}
fn check_sources(
    config: &Config,
    work: &Path,
) -> Result<(Plan, String, BTreeMap<String, Snapshot>)> {
    let (plan, input) = Plan::decode(&fs::read(work.join("input.json"))?)?;
    let mut sources = BTreeMap::new();
    let public = PublicKey::new(config, "rollback-sources")?;
    for identity in plan.source_ids() {
        let directory = work.join("sources").join(identity);
        let mut snapshot =
            Snapshot::decode(&fs::read(directory.join("manifest.json"))?, Some(identity))?;
        let root = directory.join("files");
        let present = files(&root)?;
        for (name, path) in &present {
            ensure!(
                !name.ends_with(".deb")
                    && !name.ends_with(".rpm")
                    && snapshot.entries.get(name) == Some(&Entry::file(path)?),
                "Rollback source differs from metadata manifest"
            );
        }
        ensure!(
            snapshot
                .entries
                .keys()
                .filter(|n| mutable(n))
                .all(|n| present.contains_key(n)),
            "Missing mutable rollback metadata"
        );
        if identity == plan.active || identity == plan.target {
            verify_metadata(&snapshot, &root, &public)?;
        } else {
            for (name, path) in &present {
                if name.ends_with("/Release") {
                    let suite = path.parent().context("Missing suite")?;
                    public.verify_apt_release(suite)?;
                } else if name.ends_with("/repomd.xml") {
                    public.verify(
                        &PathBuf::from(format!("{}.asc", path.display())),
                        Some(path),
                    )?;
                }
            }
        }
        snapshot.source = root;
        sources.insert(identity.to_owned(), snapshot);
    }
    let selected = &sources[&plan.target];
    let names: BTreeSet<_> = selected.entries.keys().filter(|n| mutable(n)).collect();
    for source in sources.values() {
        ensure!(
            source
                .entries
                .keys()
                .filter(|n| mutable(n))
                .collect::<BTreeSet<_>>()
                == names,
            "Rollback across layout changes requires a separate migration"
        );
    }
    let active = &sources[&plan.active];
    ensure!(
        selected
            .entries
            .iter()
            .filter(|(n, _)| !mutable(n))
            .all(|(n, e)| active.entries.get(n) == Some(e)),
        "Selection absent from active immutable history"
    );
    Ok((plan, input, sources))
}
fn expected_entries(plan: &Plan, sources: &BTreeMap<String, Snapshot>) -> BTreeMap<String, Entry> {
    let mut entries = sources[&plan.target].entries.clone();
    entries.extend(
        sources[&plan.active]
            .entries
            .iter()
            .filter(|(n, _)| !mutable(n))
            .map(|(n, e)| (n.clone(), e.clone())),
    );
    entries
}
pub(super) fn prepare(config: &Config, work: &Path) -> Result<String> {
    let (plan, input, sources) = check_sources(config, work)?;
    let selected = &sources[&plan.target];
    let root = work.join("result/files");
    ensure!(
        !work.join("result").exists(),
        "Prepared rollback already exists"
    );
    fs::create_dir_all(&root)?;
    for (name, path) in files(&selected.source)? {
        copy(&path, &root.join(name))?;
    }
    let mut entries = expected_entries(&plan, &sources);
    {
        let signer = Signer::new(config, "rollback-signing")?;
        for name in selected.entries.keys().filter(|n| n.ends_with("/Release")) {
            let previous = sources
                .values()
                .map(|s| fs::read(s.source.join(name)))
                .collect::<std::io::Result<Vec<_>>>()?;
            let date = next_release_date(&previous, Utc::now())?;
            let release = root.join(name);
            let suite = release.parent().context("Missing suite")?;
            fs::write(&release, dated_release(&fs::read(&release)?, &date)?)?;
            signer.sign(&release, true, &suite.join("InRelease"))?;
            signer.sign(&release, false, &suite.join("Release.gpg"))?;
        }
    }
    for (name, path) in files(&root)? {
        entries.insert(name, Entry::file(&path)?);
    }
    let snapshot = Snapshot::new(entries, Some(plan.active), Some(input), root)?;
    write(&work.join("result/manifest.json"), &snapshot.data)?;
    copy(&work.join("input.json"), &work.join("result/input.json"))?;
    check_result(config, work)?;
    Ok(snapshot.id)
}
pub(super) fn check_result(config: &Config, work: &Path) -> Result<(Plan, Snapshot)> {
    let (plan, input, sources) = check_sources(config, work)?;
    let snapshot = Snapshot::local(&work.join("result/files"))?;
    ensure!(
        snapshot.input.as_deref() == Some(&input)
            && snapshot.base.as_deref() == Some(&plan.active)
            && fs::read(work.join("result/input.json"))? == fs::read(work.join("input.json"))?,
        "Signed rollback belongs to another request"
    );
    let expected = expected_entries(&plan, &sources);
    ensure!(
        snapshot.entries.keys().eq(expected.keys()),
        "Rollback changed file set"
    );
    for (name, entry) in expected {
        if !["/Release", "/InRelease", "/Release.gpg"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            ensure!(
                snapshot.entries[&name] == entry,
                "Rollback changed package bytes or indexes"
            );
        }
        if name.ends_with("/Release") {
            let data = fs::read(snapshot.source.join(&name))?;
            let timestamp = release_date(&data)?;
            for source in sources.values() {
                ensure!(
                    timestamp > release_date(&fs::read(source.source.join(&name))?)?,
                    "Rollback date must exceed every exposed root"
                );
            }
            ensure!(
                timestamp <= Utc::now() + chrono::Duration::seconds(5),
                "Rollback dated too far ahead"
            );
            ensure!(
                dated_release(&data, "DATE")?
                    == dated_release(&fs::read(sources[&plan.target].source.join(&name))?, "DATE")?,
                "Rollback changed apt identity or indexes"
            );
        }
    }
    let public = PublicKey::new(config, "rollback-result")?;
    verify_metadata(&snapshot, &snapshot.source, &public)?;
    Ok((plan, snapshot))
}
pub(super) fn publish(config: &Config, work: &Path) -> Result<()> {
    let (plan, snapshot) = check_result(config, work)?;
    let store = S3Store::new(config)?;
    let mut publisher = Publisher::new(&store, config);
    for id in plan.source_ids() {
        ensure!(
            publisher.archived(id)?.data
                == fs::read(work.join("sources").join(id).join("manifest.json"))?,
            "Rollback source differs from saved archive"
        );
    }
    let input = snapshot
        .input
        .as_deref()
        .context("Missing rollback identity")?;
    save_bundle(&publisher, work, &bundle_key(config, input))?;
    publisher.ensure_bytes(
        &request_key(config, input),
        &fs::read(work.join("input.json"))?,
    )?;
    publisher.publish(&snapshot, Some((&plan.active, plan.pending.as_deref())))?;
    println!("Published fresh rollback snapshot {}", snapshot.id);
    Ok(())
}
