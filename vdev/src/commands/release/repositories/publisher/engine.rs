//! Publication phases shared by the protected CI jobs.
use super::{
    bundles::{checked_download, pack_bundle, unpack_bundle},
    inputs::{client, verify_input},
    model::{Config, Entry, LIMIT, Snapshot, files, mutable, string, write},
    repository::{PublicKey, lint_package, repomd_locations, verify_metadata, verify_rpm},
    storage::{Publisher, S3Store, Store},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, fs, io::Read, path::Path, time::Duration};

fn bundle_key(config: &Config, input: &str, base: Option<&str>) -> String {
    format!(
        "{}bundles/{input}/{}.tar.gz",
        config.bundle_prefix,
        base.unwrap_or("initial")
    )
}
fn rpm_cache_key(config: &Config, asset: &Value) -> Result<String> {
    Ok(format!(
        "{}rpms/{}.rpm",
        config.bundle_prefix,
        string(asset, "sha256")?
    ))
}
pub(super) fn download_metadata(
    publisher: &Publisher<'_>,
    snapshot: &Snapshot,
    destination: &Path,
    indexes: bool,
) -> Result<()> {
    fs::create_dir_all(destination)?;
    for (name, entry) in &snapshot.entries {
        if mutable(name) {
            checked_download(
                publisher.store,
                &format!(
                    "{}{}/files/{name}",
                    publisher.config.archive_prefix, snapshot.id
                ),
                &destination.join(name),
                Some(entry),
            )?;
        }
    }
    if indexes {
        for name in snapshot
            .entries
            .keys()
            .filter(|n| n.ends_with("/repomd.xml"))
        {
            let path = destination.join(name);
            let root = path
                .parent()
                .and_then(Path::parent)
                .context("Invalid RPM root")?;
            for relative in repomd_locations(root)? {
                let path = root.join(relative);
                let name = path
                    .strip_prefix(destination)?
                    .to_str()
                    .context("Invalid index path")?;
                let entry = snapshot
                    .entries
                    .get(name)
                    .context("RPM metadata absent from manifest")?;
                checked_download(
                    publisher.store,
                    &format!("{}{name}", publisher.config.live_prefix),
                    &path,
                    Some(entry),
                )?;
            }
        }
    }
    Ok(())
}
pub(super) fn base(config: &Config, work: &Path) -> Result<()> {
    let (manifest, input_id) = verify_input(work)?;
    let store = S3Store::new(config)?;
    let publisher = Publisher::new(&store, config);
    let (state, _) = publisher.load_state()?;
    ensure!(!state.held, "Publisher locked; inspect status");
    let active = state
        .active
        .as_deref()
        .map(|id| publisher.archived(id))
        .transpose()?;
    ensure!(
        active.is_some() || !config.require_existing_repository,
        "Historical backfill required before publication"
    );
    let candidate = if let Some(pending) = &state.pending {
        let snapshot = publisher.archived(pending)?;
        ensure!(
            snapshot.input.as_deref() == Some(&input_id),
            "Resume or roll back pending publication"
        );
        Some(snapshot)
    } else {
        active
            .as_ref()
            .filter(|s| s.input.as_deref() == Some(&input_id))
            .cloned()
    };
    let base = candidate.as_ref().map_or_else(
        || active.as_ref().map(|s| s.id.as_str()),
        |s| s.base.as_deref(),
    );
    let key = bundle_key(config, &input_id, base);
    if store.head(&key)?.is_some() {
        let archive = work.join("cached.tar.gz");
        checked_download(&store, &key, &archive, None)?;
        let snapshot = unpack_bundle(&archive, &work.join("result"))?;
        ensure!(
            snapshot.input.as_deref() == Some(&input_id)
                && snapshot.base.as_deref() == base
                && fs::read(work.join("result/input.json"))? == fs::read(work.join("input.json"))?,
            "Cached publication belongs to another input/base"
        );
        fs::remove_file(archive)?;
        println!("Restored saved signed publication; signing skipped");
        return Ok(());
    }
    ensure!(
        candidate.is_none(),
        "Required signed retry bundle is missing"
    );
    write(
        &work.join("base.json"),
        active
            .as_ref()
            .map_or(b"null\n".as_slice(), |s| s.data.as_slice()),
    )?;
    let destination = work.join("base");
    fs::create_dir(&destination)?;
    if let Some(active) = active {
        download_metadata(&publisher, &active, &destination, true)?;
    }
    let reuse = work.join("reuse");
    fs::create_dir(&reuse)?;
    for (name, asset) in manifest["assets"].as_object().context("Missing assets")? {
        if asset["format"] == "rpm" {
            let key = rpm_cache_key(config, asset)?;
            if store.head(&key)?.is_some() {
                checked_download(&store, &key, &reuse.join(name), None)?;
            }
        }
    }
    println!("Downloaded current metadata and saved RPM signatures");
    Ok(())
}
pub(super) fn check_result(config: &Config, work: &Path) -> Result<(Value, Snapshot)> {
    let (manifest, input_id) = verify_input(work)?;
    let snapshot = Snapshot::local(&work.join("result/files"))?;
    ensure!(
        snapshot.input.as_deref() == Some(&input_id)
            && fs::read(work.join("result/input.json"))? == fs::read(work.join("input.json"))?,
        "Signed output belongs to another input"
    );
    let public = PublicKey::new(config, "check-result")?;
    verify_metadata(&snapshot, &snapshot.source, &public)?;
    let rpmdb = public.rpmdb()?;
    for (name, asset) in manifest["assets"].as_object().context("Missing assets")? {
        if asset["format"] == "deb" {
            ensure!(
                snapshot.entries.get(&format!("apt/pool/v/ve/{name}"))
                    == Some(&super::model::entry_value(asset)?),
                "Debian package changed"
            );
        } else if asset["format"] == "rpm" {
            let path = snapshot.source.join(format!(
                "yum/{}/vector-0/{}/{name}",
                string(&manifest, "channel")?,
                string(asset, "arch")?
            ));
            lint_package(&path, asset)?;
            verify_rpm(&rpmdb, &path)?;
        }
    }
    Ok((manifest, snapshot))
}
pub(super) fn save_bundle(publisher: &Publisher<'_>, work: &Path, key: &str) -> Result<()> {
    let archive = work.join("prepared.tar.gz");
    if archive.exists() {
        fs::remove_file(&archive)?;
    }
    pack_bundle(&work.join("result"), &archive)?;
    let entry = Entry::file(&archive)?;
    ensure!(
        entry.size < LIMIT,
        "Prepared bundle exceeds single-PUT limit"
    );
    if let Some(saved) = publisher.store.head(key)? {
        if saved.entry == entry {
            return Ok(());
        }
        // Compression or tar encodings can change between tool versions. Keep
        // the saved archive when every signed file and manifest is identical.
        let temporary = tempfile::tempdir_in(work)?;
        let saved_archive = temporary.path().join("saved.tar.gz");
        checked_download(publisher.store, key, &saved_archive, Some(&saved.entry))?;
        let saved_result = temporary.path().join("result");
        unpack_bundle(&saved_archive, &saved_result)?;
        let contents = |root: &Path| -> Result<Vec<(String, Entry)>> {
            files(root)?
                .into_iter()
                .map(|(name, path)| Ok((name, Entry::file(&path)?)))
                .collect()
        };
        ensure!(
            contents(&saved_result)? == contents(&work.join("result"))?,
            "Saved signed bundle contains different files"
        );
        return Ok(());
    }
    publisher.ensure_file(key, &archive, &entry)
}
pub(super) fn publish(config: &Config, work: &Path) -> Result<()> {
    let (manifest, snapshot) = check_result(config, work)?;
    let store = S3Store::new(config)?;
    let mut publisher = Publisher::new(&store, config);
    ensure!(
        publisher.load_state()?.0.active.is_some() || !config.require_existing_repository,
        "Historical backfill required"
    );
    for (name, asset) in manifest["assets"].as_object().context("Missing assets")? {
        if asset["format"] == "rpm" {
            let relative = format!(
                "yum/{}/vector-0/{}/{name}",
                string(&manifest, "channel")?,
                string(asset, "arch")?
            );
            publisher.ensure_file(
                &rpm_cache_key(config, asset)?,
                &snapshot.source.join(&relative),
                &snapshot.entries[&relative],
            )?;
        }
    }
    save_bundle(
        &publisher,
        work,
        &bundle_key(
            config,
            snapshot
                .input
                .as_deref()
                .context("Missing input identity")?,
            snapshot.base.as_deref(),
        ),
    )?;
    publisher.publish(&snapshot, None)?;
    println!("Published snapshot {}", snapshot.id);
    Ok(())
}
pub(super) fn verify_snapshot(config: &Config, snapshot: &Snapshot) -> Result<()> {
    verify_endpoints(config, snapshot, &client(true)?)
}
pub(super) fn verify_endpoints(
    config: &Config,
    snapshot: &Snapshot,
    client: &reqwest::blocking::Client,
) -> Result<()> {
    let mut names: BTreeSet<_> = snapshot
        .entries
        .keys()
        .filter(|n| mutable(n))
        .cloned()
        .collect();
    names.extend(files(&snapshot.source)?.into_keys());
    for name in &names {
        let (repository, relative) = name.split_once('/').context("Invalid repository path")?;
        let root = if repository == "apt" {
            &config.apt_url
        } else {
            &config.yum_url
        };
        ensure!(
            root.starts_with("https://") && root.ends_with('/'),
            "Verification requires HTTPS repository roots"
        );
        let expected = &snapshot.entries[name];
        // Newly published objects can still be stale or missing at a CDN edge.
        // Retry the complete download/check; success still requires exact bytes.
        for attempt in 0..7 {
            let result = (|| -> Result<()> {
                let response = client
                    .get(format!("{root}{relative}"))
                    .send()?
                    .error_for_status()?;
                ensure!(
                    response.url().scheme() == "https",
                    "Repository redirected away from HTTPS"
                );
                ensure!(
                    Entry::read(response.take(expected.size + 1))? == *expected,
                    "Endpoint differs from snapshot: {name}"
                );
                Ok(())
            })();
            match result {
                Ok(()) => break,
                Err(error) if attempt < 6 => {
                    let delay = 1 << attempt;
                    println!("Verification of {name} failed: {error:#}; retrying in {delay}s");
                    std::thread::sleep(Duration::from_secs(delay));
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Verification of {name} failed after 7 attempts")
                    });
                }
            }
        }
    }
    println!(
        "Verified {} endpoint objects for {}",
        names.len(),
        snapshot.id
    );
    Ok(())
}
