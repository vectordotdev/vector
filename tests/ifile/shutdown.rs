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
    assert!(!first.is_empty() && first.len() < expected.len());
    assert_eq!(first, expected[..first.len()]);
    assert_eq!(fixture.checkpoint_position()?, lines(&first).len() as u64);
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(expected.len() - first.len()).await?;
    let second = run.stop(Signal::SIGTERM).await?.messages;
    assert_eq!([first, second].concat(), expected);
    Ok(())
}
