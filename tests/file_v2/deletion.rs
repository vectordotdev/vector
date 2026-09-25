use std::time::Duration;

use nix::sys::signal::Signal;
use serde_json::json;

use super::harness::Fixture;

#[tokio::test]
async fn deletion_retains_small_files_until_they_can_be_fingerprinted() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("small.log", &["small record".to_owned()])?;
    let path = fixture.input.join("small.log");
    let mut run = fixture.start("*.log", json!({"remove_after_secs": 1}))?;
    run.wait_for("source readiness", |seen| seen.open_files == Some(0.0))
        .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(path.exists(), "unread file was deleted");
    assert!(run.stop(Signal::SIGTERM).await?.messages.is_empty());

    // The documented workaround allows this completed small file to be consumed.
    let mut run = fixture.start(
        "*.log",
        json!({
            "remove_after_secs": 1, "fingerprint": {"strategy": "checksum", "bytes": 4}
        }),
    )?;
    run.wait_count(1).await?;
    run.wait_for("acknowledged file deletion", |_| !path.exists())
        .await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, ["small record"]);
    Ok(())
}

#[tokio::test]
async fn deletion_retains_unfinished_lines() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.input.join("partial.log");
    std::fs::write(&path, "first\nunfinished")?;
    let mut run = fixture.start(
        "*.log",
        json!({
            "remove_after_secs": 1, "fingerprint": {"strategy": "checksum", "bytes": 4}
        }),
    )?;
    run.wait_count(1).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(path.exists(), "file with a partial record was deleted");
    fixture.append("partial.log", &[" completed".to_owned()])?;
    run.wait_count(2).await?;
    run.wait_for("completed file deletion", |_| !path.exists())
        .await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        ["first", "unfinished completed"]
    );
    Ok(())
}

#[tokio::test]
async fn deletion_waits_for_multiline_flush() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.input.join("multiline.log");
    fixture.write("multiline.log", &["INFO pending".to_owned()])?;
    let mut run = fixture.start(
        "*.log",
        json!({
            "remove_after_secs": 1,
            "fingerprint": {"strategy": "checksum", "bytes": 4},
            "multiline": {
                "start_pattern": "^INFO", "condition_pattern": "^INFO",
                "mode": "halt_before", "timeout_ms": 5000
            }
        }),
    )?;
    run.wait_for("open file", |seen| seen.open_files == Some(1.0))
        .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(path.exists(), "deleted before multiline flush");
    run.wait_count(1).await?;
    run.wait_for("flushed file deletion", |_| !path.exists())
        .await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, ["INFO pending"]);
    Ok(())
}

#[tokio::test]
async fn deletion_of_file_started_at_end() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("old.log", &["old contents".into()])?;
    let path = fixture.input.join("old.log");
    let mut run = fixture.start("*.log", json!({"read_from": "end", "remove_after_secs": 1, "fingerprint": {"strategy": "checksum", "bytes": 4}}))?;
    run.wait_for("skipped file deletion", |_| !path.exists())
        .await?;
    assert!(run.stop(Signal::SIGTERM).await?.messages.is_empty());
    Ok(())
}

#[tokio::test]
async fn terminal_oversized_record_allows_checkpoint_and_deletion() -> vector::Result<()> {
    for acknowledgements in [true, false] {
        let fixture = Fixture::new()?;
        let path = fixture.input.join("large.log");
        let input = format!("good\n{}\n", "x".repeat(1024));
        std::fs::write(&path, &input)?;
        let mut run = fixture.start("*.log", json!({"max_line_bytes": 8, "max_read_bytes": 16, "remove_after_secs": 1, "acknowledgements": {"enabled": acknowledgements}, "fingerprint": {"strategy": "checksum", "bytes": 4}}))?;
        run.wait_count(1).await?;
        run.wait_for("terminal discarded record deletion", |_| !path.exists())
            .await?;
        assert_eq!(run.stop(Signal::SIGTERM).await?.messages, ["good"]);
        assert_eq!(fixture.checkpoint_position()?, input.len() as u64);
    }
    Ok(())
}
