use std::os::unix::fs::symlink;

use nix::sys::signal::Signal;
use serde_json::json;

use super::{
    common::{lines, records},
    harness::Fixture,
};

#[tokio::test]
async fn excluded_symlink_is_not_ingested() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("public", 1);
    fixture.write("public.log", &expected)?;
    let target = fixture.input.parent().unwrap().join("private.txt");
    std::fs::write(&target, lines(&records("excluded", 1)))?;
    let excluded = fixture.input.join("private.log");
    symlink(&target, &excluded)?;
    let mut run = fixture.start("*.log", json!({"exclude": [excluded]}))?;
    run.wait_count(expected.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, expected);
    Ok(())
}

#[tokio::test]
async fn exclusions_still_apply_through_a_symlinked_directory() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let target = fixture.input.parent().unwrap().join("logs");
    std::fs::create_dir(&target)?;
    symlink(&target, fixture.input.join("alias"))?;
    let expected = records("public", 1);
    fixture.write("alias/public.log", &expected)?;
    fixture.write("alias/private.log", &records("excluded", 1))?;
    let mut run = fixture.start(
        "alias/*.log",
        json!({
            "exclude": [fixture.input.join("alias/private.log")]
        }),
    )?;
    run.wait_count(expected.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, expected);
    Ok(())
}
