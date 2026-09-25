use super::harness::Fixture;
use nix::sys::signal::Signal;
use serde_json::json;
use std::{fs::OpenOptions, io::Write};

#[tokio::test]
async fn oversized_records_emit_one_error_and_drop_even_before_the_delimiter() -> vector::Result<()>
{
    let fixture = Fixture::new()?;
    let path = fixture.input.join("large.log");
    std::fs::write(&path, format!("good\n{}", "x".repeat(1024)))?;
    let mut run = fixture.start(
        "*.log",
        json!({
            "fingerprint": {"strategy": "checksum", "bytes": 4},
            "max_line_bytes": 8, "max_read_bytes": 16
        }),
    )?;
    run.wait_count(1).await?;
    run.wait_for("unterminated oversized record reported", |seen| {
        seen.oversized_errors == 1.0 && seen.discarded_events == 1.0
    })
    .await?;
    let mut writer = OpenOptions::new().append(true).open(&path)?;
    writer.write_all(format!("{}\nok\n{}\ngo\n", "x".repeat(8192), "y".repeat(4096)).as_bytes())?;
    writer.sync_all()?;
    run.wait_count(3).await?;
    run.wait_for(
        "exactly one report per oversized record across budgets",
        |seen| seen.oversized_errors == 2.0 && seen.discarded_events == 2.0,
    )
    .await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    assert_eq!(seen.messages, ["good", "ok", "go"]);
    assert_eq!(seen.oversized_errors, 2.0);
    assert_eq!(seen.discarded_events, 2.0);
    Ok(())
}

#[tokio::test]
async fn retiring_partial_delimiter_reports_an_oversized_tail() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.input.join("partial.log");
    std::fs::write(&path, "first\r\n12345678\r")?;
    let mut run = fixture.start(
        "*.log",
        json!({
            "fingerprint": {"strategy": "checksum", "bytes": 4},
            "max_line_bytes": 8, "line_delimiter": "\r\n", "reader_idle_timeout_secs": 0
        }),
    )?;
    run.wait_count(1).await?;
    std::fs::rename(&path, fixture.input.join("retired.1"))?;
    run.wait_for("oversized unfinished tail reported at retirement", |seen| {
        seen.open_files == Some(0.0) && seen.oversized_errors == 1.0 && seen.discarded_events == 1.0
    })
    .await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, ["first"]);
    Ok(())
}
