use nix::sys::signal::Signal;
use serde_json::json;

use super::{common::records, harness::Fixture};

#[tokio::test]
async fn backlog_does_not_starve_new_small_file() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let backlog = records("backlog", 2000);
    let small = records("small", 1);
    fixture.write("large.log", &backlog)?;
    let mut run = fixture.start(
        "*.log",
        json!({"max_read_bytes": 64, "oldest_first": false}),
    )?;
    run.wait_count(1).await?;
    assert!(run.observed.messages.len() < backlog.len());
    fixture.write("small.log", &small)?;
    run.wait_for("small file while backlog remains", |seen| {
        seen.messages.contains(&small[0])
    })
    .await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    let small_position = seen
        .messages
        .iter()
        .position(|line| line == &small[0])
        .unwrap();
    assert!(
        small_position < backlog.len(),
        "small file waited for the entire backlog"
    );
    assert_eq!(
        seen.messages
            .iter()
            .filter(|line| *line == &small[0])
            .count(),
        1
    );
    let large: Vec<_> = seen
        .messages
        .into_iter()
        .filter(|line| line != &small[0])
        .collect();
    assert!(
        large.len() < backlog.len(),
        "shutdown drained the entire backlog"
    );
    assert_eq!(large, backlog[..large.len()]);
    Ok(())
}
