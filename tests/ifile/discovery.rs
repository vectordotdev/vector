use nix::sys::signal::Signal;
use serde_json::json;

use super::{common::records, harness::Fixture};

#[tokio::test]
async fn default_fingerprint_distinguishes_files_with_the_same_first_line() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let first = [vec!["Starting service".to_owned()], records("first", 2)].concat();
    let second = [vec!["Starting service".to_owned()], records("second", 2)].concat();
    fixture.write("first.log", &first)?;
    fixture.write("second.log", &second)?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(first.len() + second.len()).await?;
    let mut actual = run.stop(Signal::SIGTERM).await?.messages;
    let mut expected = [first, second].concat();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);

    // Distinct fingerprints must also preserve independent checkpoints.
    let appended = vec![
        "new first record".to_owned(),
        "new second record".to_owned(),
    ];
    fixture.append("first.log", &appended[..1])?;
    fixture.append("second.log", &appended[1..])?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(2).await?;
    let mut actual = run.stop(Signal::SIGTERM).await?.messages;
    actual.sort();
    assert_eq!(actual, appended);
    Ok(())
}

#[tokio::test]
async fn configured_fingerprint_waits_for_the_full_prefix() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let initial = vec!["same".to_owned()]; // Five bytes including the newline.
    fixture.write("small.log", &initial)?;
    let mut run = fixture.start(
        "*.log",
        json!({"fingerprint": {"strategy": "checksum", "bytes": 8}}),
    )?;
    run.wait_for("small file is not opened", |seen| {
        seen.open_files == Some(0.0)
    })
    .await?;
    assert!(run.observed.messages.is_empty());
    let appended = vec!["abc".to_owned()];
    fixture.append("small.log", &appended)?;
    run.wait_count(2).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [initial, appended].concat()
    );
    Ok(())
}

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

#[tokio::test]
async fn discovers_files_when_watch_directory_is_created_later() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let mut run = fixture.start("missing/*.log", json!({}))?;
    run.wait_for("empty source readiness", |seen| {
        seen.open_files == Some(0.0)
    })
    .await?;
    std::fs::create_dir(fixture.input.join("missing"))?;
    let expected = records("late-directory", 3);
    fixture.write("missing/new.log", &expected)?;
    run.wait_count(expected.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, expected);
    Ok(())
}
