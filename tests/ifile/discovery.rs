use nix::sys::signal::Signal;
use serde_json::json;

use super::{common::records, harness::Fixture};

#[tokio::test]
async fn discovers_files_before_and_after_startup() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.input.join("existing"))?;
    let expected = records("discovery", 3);
    fixture.write("existing/before.log", &expected[..1])?;
    let mut run = fixture.start("*/*.log", json!({}))?;
    run.wait_count(1).await?;
    fixture.write("existing/after.log", &expected[1..2])?;
    run.wait_count(2).await?;
    std::fs::create_dir(fixture.input.join("new-directory"))?;
    fixture.write("new-directory/new.log", &expected[2..])?;
    run.wait_count(3).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    assert_eq!(seen.messages, expected);
    assert_eq!(seen.received_events, 3.0);
    Ok(())
}
