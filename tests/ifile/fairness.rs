use nix::sys::signal::Signal;
use serde_json::json;

use super::{common::records, harness::Fixture};

#[tokio::test]
async fn read_batches_allow_other_files_to_progress() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let first = records("first-backlog", 1000);
    let second = records("second-backlog", 1000);
    fixture.write("first.log", &first)?;
    fixture.write("second.log", &second)?;
    let mut run = fixture.start(
        "*.log",
        json!({"max_read_bytes": 64, "oldest_first": false}),
    )?;
    run.wait_count(first.len() + second.len()).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    // Both files are ready before startup. Regardless of discovery order, neither
    // may be drained before the other gets a turn. This does not depend on speed.
    for expected in [&first, &second] {
        let position = seen
            .messages
            .iter()
            .position(|line| line == &expected[0])
            .unwrap();
        assert!(
            position < expected.len(),
            "a file waited for the other backlog to drain"
        );
        let actual: Vec<_> = seen
            .messages
            .iter()
            .filter(|line| expected.contains(line))
            .cloned()
            .collect();
        assert_eq!(&actual, expected);
    }
    Ok(())
}
