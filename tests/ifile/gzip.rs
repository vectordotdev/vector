use flate2::{Compression, write::GzEncoder};
use nix::sys::signal::Signal;
use serde_json::json;
use std::{fs::File, io::Write};

use super::{
    common::{lines, records},
    harness::Fixture,
};

#[tokio::test]
async fn gzip_exact_output_across_read_batches() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("gzip", 30);
    let mut gzip = GzEncoder::new(
        File::create(fixture.input.join("compressed.log"))?,
        Compression::default(),
    );
    gzip.write_all(lines(&expected).as_bytes())?;
    gzip.finish()?.sync_all()?;
    let mut run = fixture.start("*.log", json!({"max_read_bytes": 1}))?;
    run.wait_count(expected.len()).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    assert_eq!(seen.messages, expected);
    assert_eq!(seen.received_events, 30.0);
    Ok(())
}
