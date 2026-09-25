use nix::sys::signal::Signal;
use serde_json::json;

use super::{
    common::{lines, records},
    harness::Fixture,
};

#[tokio::test]
async fn graceful_restart_resumes_exactly() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("graceful-before", 10);
    let after = records("graceful-after", 10);
    fixture.write("active.log", &before)?;
    let options = json!({"checkpoint_interval": 3_600_000});
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(before.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, before);
    assert_eq!(fixture.checkpoint_position()?, lines(&before).len() as u64);
    fixture.append("active.log", &after)?;
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(after.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, after);
    assert_eq!(
        fixture.checkpoint_position()?,
        lines(&[before, after].concat()).len() as u64
    );
    Ok(())
}

#[tokio::test]
async fn abrupt_restart_replays_only_since_checkpoint_without_loss() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let committed = records("committed", 3);
    let pending = records("pending", 100);
    let unread = records("after-crash", 10);
    let options = json!({"checkpoint_interval": 3_600_000, "max_read_bytes": 1});
    fixture.write("active.log", &committed)?;
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(committed.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, committed);
    let offset = fixture.checkpoint_position()?;
    assert_eq!(offset, lines(&committed).len() as u64);

    fixture.append("active.log", &pending)?;
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(3).await?;
    let interrupted = run.stop(Signal::SIGKILL).await?.messages;
    assert!(!interrupted.is_empty() && interrupted.len() <= pending.len());
    assert_eq!(interrupted, pending[..interrupted.len()]);
    assert_eq!(fixture.checkpoint_position()?, offset);

    // Append while Vector is stopped so unread data does not depend on how
    // quickly the process handles SIGKILL after we observe its output.
    fixture.append("active.log", &unread)?;
    let expected = [pending, unread].concat();

    // Documented guarantee: an abrupt stop can replay uncheckpointed data.
    // Require every pending record, but no replay from before the durable checkpoint.
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(expected.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, expected);
    Ok(())
}
