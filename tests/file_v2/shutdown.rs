use nix::sys::signal::Signal;
use serde_json::json;

use super::{
    common::{lines, records},
    harness::Fixture,
};

#[tokio::test]
async fn shutdown_saves_final_acknowledged_offset_before_exit() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("shutdown", 200);
    fixture.write("active.log", &expected)?;
    let options = json!({"max_read_bytes": 1, "checkpoint_interval": 3_600_000});
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(3).await?;
    let first = run.stop(Signal::SIGTERM).await?.messages;
    assert!(!first.is_empty() && first.len() <= expected.len());
    assert_eq!(first, expected[..first.len()]);
    assert_eq!(fixture.checkpoint_position()?, lines(&first).len() as u64);
    // There may be unread data, but a fast reader can finish before SIGTERM.
    // New data written while stopped guarantees that restart also reads forward.
    let appended = records("after-shutdown", 10);
    fixture.append("active.log", &appended)?;
    let expected = [expected, appended].concat();
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(expected.len() - first.len()).await?;
    let second = run.stop(Signal::SIGTERM).await?.messages;
    assert_eq!([first, second].concat(), expected);
    Ok(())
}

#[tokio::test]
async fn empty_source_shuts_down_on_single_worker() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_for("empty source readiness", |seen| {
        seen.open_files == Some(0.0)
    })
    .await?;
    assert!(run.stop(Signal::SIGTERM).await?.messages.is_empty());
    Ok(())
}

#[tokio::test]
async fn notification_burst_preserves_output_and_allows_shutdown() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let mut run = fixture.start("*.log", json!({"max_read_bytes": 64}))?;
    run.wait_for("empty source readiness", |seen| {
        seen.open_files == Some(0.0)
    })
    .await?;
    let mut expected = Vec::new();
    for index in 0..128 {
        let lines = records(&format!("burst-{index}"), 4);
        fixture.write(&format!("{index}.log"), &lines)?;
        expected.extend(lines);
    }
    run.wait_count(expected.len()).await?;
    // Repeated writes create another burst against already-open readers.
    for index in 0..128 {
        let lines = records(&format!("append-{index}"), 4);
        fixture.append(&format!("{index}.log"), &lines)?;
        expected.extend(lines);
    }
    run.wait_count(expected.len()).await?;
    let mut actual = run.stop(Signal::SIGTERM).await?.messages;
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    Ok(())
}
