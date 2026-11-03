//! Bounded, checksum-verified transport of signed workspaces.
use super::{
    model::{Entry, LIMIT, Snapshot, files, safe_relative},
    storage::Store,
};
use anyhow::{Result, ensure};
use flate2::{Compression, GzBuilder};
use std::{collections::BTreeSet, fs, io::Write, path::Path};
pub(super) fn checked_download(
    store: &dyn Store,
    key: &str,
    path: &Path,
    entry: Option<&Entry>,
) -> Result<()> {
    let head = store
        .head(key)?
        .ok_or_else(|| anyhow::anyhow!("Missing S3 input: {key}"))?;
    ensure!(head.entry.size < LIMIT, "Oversized S3 input");
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Missing parent"))?,
    )?;
    let actual = store.download(key, path, head.entry.size)?;
    ensure!(
        head.entry == actual && entry.is_none_or(|e| *e == actual),
        "Downloaded S3 checksum mismatch"
    );
    Ok(())
}
pub(super) fn unpack_bundle(path: &Path, destination: &Path) -> Result<Snapshot> {
    fs::create_dir(destination)?;
    let mut archive = tar::Archive::new(super::model::gzip(fs::File::open(path)?, LIMIT));
    let mut names = BTreeSet::new();
    let mut total: u64 = 0;
    for member in archive.entries()? {
        let mut member = member?;
        let name = member
            .path()?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF8 archive name"))?
            .to_owned();
        ensure!(
            member.header().entry_type().is_file() && names.insert(name.clone()),
            "Duplicate or nonregular bundle member"
        );
        if let Some(relative) = name.strip_prefix("files/") {
            safe_relative(relative)?;
        } else {
            ensure!(
                matches!(name.as_str(), "input.json" | "manifest.json"),
                "Unexpected bundle member"
            );
        }
        total = total
            .checked_add(member.size())
            .ok_or_else(|| anyhow::anyhow!("Archive size overflow"))?;
        ensure!(total < LIMIT, "Bundle extraction budget exceeded");
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        std::io::copy(&mut member, &mut output)?;
        output.flush()?;
    }
    ensure!(
        names.contains("input.json") && names.contains("manifest.json"),
        "Incomplete signed bundle"
    );
    Snapshot::local(&destination.join("files"))
}
pub(super) fn pack_bundle(source: &Path, target: &Path) -> Result<()> {
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let compressed = GzBuilder::new()
        .mtime(0)
        .write(output, Compression::default());
    let mut archive = tar::Builder::new(compressed);
    for (name, path) in files(source)? {
        let mut header = tar::Header::new_gnu();
        header.set_size(fs::metadata(&path)?.len());
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        archive.append_data(&mut header, name, fs::File::open(path)?)?;
    }
    archive.into_inner()?.finish()?;
    Ok(())
}
