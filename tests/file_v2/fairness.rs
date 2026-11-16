use nix::sys::signal::Signal;
use serde_json::{Value, json};

use super::{common::records, harness::Fixture};

#[tokio::test]
async fn read_batches_allow_other_files_to_progress() -> vector::Result<()> {
    assert_fair_reads(json!({"max_read_bytes": 64}), 1).await
}

#[tokio::test]
async fn default_read_batches_allow_other_files_to_progress() -> vector::Result<()> {
    // Each fixture record exceeds 1 KiB, so a 64 KiB batch gives the other
    // ready file a turn after at most 64 records.
    assert_fair_reads(json!({}), 64).await
}

async fn assert_fair_reads(
    options: Value,
    max_records_before_other_file: usize,
) -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let first = records("first-backlog", 1000);
    let second = records("second-backlog", 1000);
    fixture.write("first.log", &first)?;
    fixture.write("second.log", &second)?;
    let mut run = fixture.start("*.log", options)?;
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
            position <= max_records_before_other_file,
            "a file waited longer than one read batch"
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

#[tokio::test]
async fn oversized_records_do_not_monopolize_file_turns() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    for name in ["a", "b"] {
        let oversized = format!("{}\n", name.repeat(1024)).repeat(128);
        std::fs::write(
            fixture.input.join(format!("{name}.log")),
            format!("start-{name}\n{oversized}end-{name}\n"),
        )?;
    }
    let mut run = fixture.start(
        "*.log",
        json!({
            "max_line_bytes": 32, "max_read_bytes": 1024
        }),
    )?;
    run.wait_count(4).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    let mut first = seen.messages[..2].to_vec();
    first.sort();
    assert_eq!(
        first,
        ["start-a", "start-b"],
        "oversized backlog monopolized a file turn"
    );
    let mut all = seen.messages;
    all.sort();
    assert_eq!(all, ["end-a", "end-b", "start-a", "start-b"]);
    Ok(())
}
