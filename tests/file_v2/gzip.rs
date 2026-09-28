use flate2::{Compression, write::GzEncoder};
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
    let run = fixture.start("*.log", json!({"max_read_bytes": 1}))?;
    let seen = run.finish_with_messages(&expected).await?;
    assert_eq!(seen.received_events, 30.0);
    Ok(())
}

#[tokio::test]
async fn gzip_appended_members_after_eof_and_rename() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.input.join("compressed.log");
    let mut first = GzEncoder::new(File::create(&path)?, Compression::default());
    first.write_all(b"first\npartial")?;
    first.finish()?.sync_all()?;
    let mut writer = fixture.open_append("compressed.log")?;
    let mut run = fixture.start(
        "*.log",
        json!({"fingerprint": {"strategy": "checksum", "bytes": 1}}),
    )?;
    // This tiny file fits in one read turn: the server reaches EOF before
    // flushing its batch, so observing the record establishes the boundary.
    run.wait_count(1).await?;
    std::fs::rename(&path, fixture.input.join("rotated.gz"))?;

    for (contents, count) in [("_tail\nsecond\n", 3), ("third\n", 4)] {
        let mut member = GzEncoder::new(Vec::new(), Compression::default());
        member.write_all(contents.as_bytes())?;
        // Materialize each complete member before appending through the
        // original handle, which remains valid after the rename.
        writer.write_all(&member.finish()?)?;
        writer.sync_all()?;
        run.wait_count(count).await?;
    }
    run.finish_with_messages(&["first", "partial_tail", "second", "third"])
        .await?;
    Ok(())
}

#[tokio::test]
async fn gzip_deletion_uses_delivery_of_decoded_records() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("gzip-deletion", 3);
    let path = fixture.input.join("compressed.log");
    let mut gzip = GzEncoder::new(File::create(&path)?, Compression::default());
    gzip.write_all(lines(&expected).as_bytes())?;
    gzip.finish()?.sync_all()?;
    let mut run = fixture.start("*.log", json!({"remove_after_secs": 1}))?;
    run.wait_count(expected.len()).await?;
    run.wait_for("gzip deletion", |_| !path.exists()).await?;
    run.finish_with_messages(&expected).await?;
    Ok(())
}
