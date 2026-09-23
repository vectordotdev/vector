use nix::sys::signal::Signal;
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write};

use super::{
    common::{lines, records},
    harness::Fixture,
};

#[tokio::test]
async fn rotation_rename_and_create() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("before-rotation", 3);
    let after = records("after-rotation", 3);
    fixture.write("active.log", &before)?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(before.len()).await?;
    std::fs::rename(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    fixture.write("active.log", &after)?;
    run.wait_count(before.len() + after.len()).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, after].concat()
    );
    Ok(())
}

#[tokio::test]
async fn rotation_reads_writes_through_old_handle() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("old-inode-before", 2);
    let late = records("old-inode-after", 2);
    let replacement = records("new-inode", 2);
    fixture.write("active.log", &before)?;
    let mut writer = OpenOptions::new()
        .append(true)
        .open(fixture.input.join("active.log"))?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(2).await?;
    // The rotated name is intentionally outside the include glob.
    std::fs::rename(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    fixture.write("active.log", &replacement)?;
    run.wait_count(4).await?;
    writer.write_all(lines(&late).as_bytes())?;
    writer.sync_all()?;
    run.wait_count(6).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, replacement, late].concat()
    );
    Ok(())
}

#[tokio::test]
async fn rotation_copy_truncate() -> vector::Result<()> {
    copy_truncate(json!({"fingerprint": {"strategy": "device_and_inode"}})).await
}

#[tokio::test]
async fn rotation_copy_truncate_checksum() -> vector::Result<()> {
    copy_truncate(json!({})).await
}

async fn copy_truncate(options: Value) -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("copy-before", 20);
    let after = records("copy-after", 3);
    fixture.write("active.log", &before)?;
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(before.len()).await?;
    std::fs::copy(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    // Rewrites the same inode, with a length smaller than its previous read offset.
    fixture.write("active.log", &after)?;
    run.wait_count(before.len() + after.len()).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, after].concat()
    );
    Ok(())
}

#[tokio::test]
async fn rotation_copy_truncate_with_unchanged_fingerprint() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    // Keep the entire fingerprint prefix unchanged: rediscovery cannot supply
    // a new identity, so the existing reader must detect the shrink and rewind.
    let header = records("shared-header", 1);
    let before = [header.clone(), records("before", 20)].concat();
    let after = [header, records("after", 3)].concat();
    fixture.write("active.log", &before)?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(before.len()).await?;
    fixture.write("active.log", &after)?;
    run.wait_count(before.len() + after.len()).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, after].concat()
    );

    let appended = records("after-restart", 1);
    fixture.append("active.log", &appended)?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(1).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, appended);
    Ok(())
}

#[tokio::test]
async fn rotation_discovers_replacement_while_draining_backlog() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let backlog = records("old-backlog", 10_000);
    let replacement = records("replacement", 3);
    let late = records("old-handle-late", 2);
    fixture.write("active.log", &backlog)?;
    let mut writer = OpenOptions::new()
        .append(true)
        .open(fixture.input.join("active.log"))?;
    let mut run = fixture.start("*.log", json!({"max_read_bytes": 64}))?;
    run.wait_for("first backlog record", |seen| !seen.messages.is_empty())
        .await?;
    std::fs::rename(
        fixture.input.join("active.log"),
        fixture.input.join("active.log.1"),
    )?;
    fixture.write("active.log", &replacement)?;
    writer.write_all(lines(&late).as_bytes())?;
    writer.sync_all()?;
    run.wait_count(backlog.len() + replacement.len() + late.len())
        .await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    let new_position = seen
        .messages
        .iter()
        .position(|line| line == &replacement[0])
        .unwrap();
    let last_old_position = seen
        .messages
        .iter()
        .position(|line| line == backlog.last().unwrap())
        .unwrap();
    assert!(
        new_position < last_old_position,
        "replacement was not discovered until the backlog drained"
    );
    // Preserve ordering within each physical file while allowing fair interleaving.
    let old: Vec<_> = seen
        .messages
        .iter()
        .filter(|line| !line.starts_with("replacement-"))
        .cloned()
        .collect();
    let new: Vec<_> = seen
        .messages
        .into_iter()
        .filter(|line| line.starts_with("replacement-"))
        .collect();
    assert_eq!(old, [backlog, late].concat());
    assert_eq!(new, replacement);
    Ok(())
}
