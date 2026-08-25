use std::{
    collections::HashSet,
    fs::{self, File},
    future::Future,
    io::{Seek, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use encoding_rs::UTF_16LE;
use indoc::indoc;
use similar_asserts::assert_eq;
use tempfile::tempdir;
use tokio::time::{Duration, sleep, timeout};
use vector_lib::schema::Definition;
use vrl::{value, value::kind::Collection};

use super::*;
use crate::{
    config::Config,
    event::{Event, EventStatus, Value},
    shutdown::ShutdownSignal,
    sources::file,
    test_util::{
        components::{FILE_SOURCE_TAGS, assert_source_compliance},
        wait_for_atomic_usize_timeout_ms,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<FileConfig>();
}

fn test_default_file_config(dir: &tempfile::TempDir) -> file::FileConfig {
    // Store checkpoints in a subdirectory so they don't appear in the
    // glob-watched directory (which covers dir.path()/*).
    let data_dir = dir.path().join(".data");
    fs::create_dir_all(&data_dir).unwrap();
    file::FileConfig {
        fingerprint: FingerprintConfig::Checksum {
            ignored_header_bytes: 0,
            lines: 1,
        },
        data_dir: Some(data_dir),
        glob_minimum_cooldown_ms: Duration::from_millis(100),
        internal_metrics: FileInternalMetricsConfig {
            include_file_tag: true,
        },
        ..Default::default()
    }
}

async fn sleep_500_millis() {
    sleep(Duration::from_millis(500)).await;
}

#[test]
fn parse_config() {
    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        file_key: file
        glob_minimum_cooldown_ms: 1000
        multi_line_timeout: 1000
        max_read_bytes: 2048
        line_delimiter: "\n"
        "#,
    })
    .unwrap();
    assert_eq!(config, FileConfig::default());
    assert_eq!(
        config.fingerprint,
        FingerprintConfig::Checksum {
            ignored_header_bytes: 0,
            lines: 1
        }
    );

    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        fingerprint:
          strategy: device_and_inode
        "#,
    })
    .unwrap();
    assert_eq!(config.fingerprint, FingerprintConfig::DevInode);

    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        fingerprint:
          strategy: checksum
          bytes: 128
          ignored_header_bytes: 512
        "#,
    })
    .unwrap();
    assert_eq!(
        config.fingerprint,
        FingerprintConfig::Checksum {
            ignored_header_bytes: 512,
            lines: 1
        }
    );

    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        encoding:
          charset: utf-16le
        "#,
    })
    .unwrap();
    assert_eq!(config.encoding, Some(EncodingConfig { charset: UTF_16LE }));

    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        read_from: beginning
        "#,
    })
    .unwrap();
    assert_eq!(config.read_from, ReadFromConfig::Beginning);

    let config: FileConfig = serde_yaml::from_str(indoc! {
        r#"
        include:
          - /var/log/**/*.log
        read_from: end
        "#,
    })
    .unwrap();
    assert_eq!(config.read_from, ReadFromConfig::End);
}

#[test]
fn resolve_data_dir() {
    let global_dir = tempdir().unwrap();
    let local_dir = tempdir().unwrap();

    let mut config = Config::default();
    config.global.data_dir = global_dir.keep().into();

    // local path given -- local should win
    let local_data_dir = Some(local_dir.path().to_path_buf());
    let res = config
        .global
        .resolve_and_validate_data_dir(local_data_dir.as_ref())
        .unwrap();
    assert_eq!(res, local_dir.path());

    // no local path given -- global fallback should be in effect
    let res = config.global.resolve_and_validate_data_dir(None).unwrap();
    assert_eq!(res, config.global.data_dir.unwrap());
}

#[test]
fn output_schema_definition_vector_namespace() {
    let definitions = FileConfig::default()
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    assert_eq!(
        definitions,
        Some(
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("file", "host"),
                    Kind::bytes().or_undefined(),
                    Some("host")
                )
                .with_metadata_field(&owned_value_path!("file", "offset"), Kind::integer(), None)
                .with_metadata_field(&owned_value_path!("file", "path"), Kind::bytes(), None)
        )
    )
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let definitions = FileConfig::default()
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    assert_eq!(
        definitions,
        Some(
            Definition::new_with_default_metadata(
                Kind::object(Collection::empty()),
                [LogNamespace::Legacy]
            )
            .with_event_field(
                &owned_value_path!("message"),
                Kind::bytes(),
                Some("message")
            )
            .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
            .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
            .with_event_field(
                &owned_value_path!("host"),
                Kind::bytes().or_undefined(),
                Some("host")
            )
            .with_event_field(&owned_value_path!("offset"), Kind::undefined(), None)
            .with_event_field(&owned_value_path!("file"), Kind::bytes(), None)
        )
    )
}

#[test]
fn create_event_legacy_namespace() {
    let line = Bytes::from("hello world");
    let file = "some_file.rs";
    let offset: u64 = 0;

    let meta = EventMetadata {
        host_key: Some(owned_value_path!("host")),
        hostname: Some("Some.Machine".to_string()),
        file_key: Some(owned_value_path!("file")),
        offset_key: Some(owned_value_path!("offset")),
    };
    let log = create_event(line, offset, file, &meta, LogNamespace::Legacy, false);

    assert_eq!(log["file"], "some_file.rs".into());
    assert_eq!(log["host"], "Some.Machine".into());
    assert_eq!(log["offset"], 0.into());
    assert_eq!(*log.get_message().unwrap(), "hello world".into());
    assert_eq!(*log.get_source_type().unwrap(), "file".into());
    assert!(log[log_schema().timestamp_key().unwrap().to_string()].is_timestamp());
}

#[test]
fn create_event_custom_fields_legacy_namespace() {
    let line = Bytes::from("hello world");
    let file = "some_file.rs";
    let offset: u64 = 0;

    let meta = EventMetadata {
        host_key: Some(owned_value_path!("hostname")),
        hostname: Some("Some.Machine".to_string()),
        file_key: Some(owned_value_path!("file_path")),
        offset_key: Some(owned_value_path!("off")),
    };
    let log = create_event(line, offset, file, &meta, LogNamespace::Legacy, false);

    assert_eq!(log["file_path"], "some_file.rs".into());
    assert_eq!(log["hostname"], "Some.Machine".into());
    assert_eq!(log["off"], 0.into());
    assert_eq!(*log.get_message().unwrap(), "hello world".into());
    assert_eq!(*log.get_source_type().unwrap(), "file".into());
    assert!(log[log_schema().timestamp_key().unwrap().to_string()].is_timestamp());
}

#[test]
fn create_event_vector_namespace() {
    let line = Bytes::from("hello world");
    let file = "some_file.rs";
    let offset: u64 = 0;

    let meta = EventMetadata {
        host_key: Some(owned_value_path!("ignored")),
        hostname: Some("Some.Machine".to_string()),
        file_key: Some(owned_value_path!("ignored")),
        offset_key: Some(owned_value_path!("ignored")),
    };
    let log = create_event(line, offset, file, &meta, LogNamespace::Vector, false);

    assert_eq!(log.value(), &value!("hello world"));

    assert_eq!(
        log.metadata()
            .value()
            .get(path!("vector", "source_type"))
            .unwrap(),
        &value!("file")
    );
    assert!(
        log.metadata()
            .value()
            .get(path!("vector", "ingest_timestamp"))
            .unwrap()
            .is_timestamp()
    );

    assert_eq!(
        log.metadata()
            .value()
            .get(path!(FileConfig::NAME, "host"))
            .unwrap(),
        &value!("Some.Machine")
    );
    assert_eq!(
        log.metadata()
            .value()
            .get(path!(FileConfig::NAME, "offset"))
            .unwrap(),
        &value!(0)
    );
    assert_eq!(
        log.metadata()
            .value()
            .get(path!(FileConfig::NAME, "path"))
            .unwrap(),
        &value!("some_file.rs")
    );
}

#[tokio::test]
async fn file_happy_path() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path1 = dir.path().join("file1");
    let path2 = dir.path().join("file2");

    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file1 = File::create(&path1).unwrap();
        let mut file2 = File::create(&path2).unwrap();

        for i in 0..n {
            writeln!(&mut file1, "hello {i}").unwrap();
            writeln!(&mut file2, "goodbye {i}").unwrap();
        }

        file1.flush().unwrap();
        file2.flush().unwrap();

        sleep_500_millis().await;
    })
    .await;

    let mut hello_i = 0;
    let mut goodbye_i = 0;

    for event in received {
        let line =
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
        if line.starts_with("hello") {
            assert_eq!(line, format!("hello {}", hello_i));
            assert_eq!(
                event.as_log()["file"].to_string_lossy(),
                path1.to_str().unwrap()
            );
            hello_i += 1;
        } else {
            assert_eq!(line, format!("goodbye {}", goodbye_i));
            assert_eq!(
                event.as_log()["file"].to_string_lossy(),
                path2.to_str().unwrap()
            );
            goodbye_i += 1;
        }
    }
    assert_eq!(hello_i, n);
    assert_eq!(goodbye_i, n);
}

// https://github.com/vectordotdev/vector/issues/8363
#[tokio::test]
async fn file_read_empty_lines() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");

    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "line for checkpointing").unwrap();
        for _i in 0..n {
            writeln!(&mut file).unwrap();
        }
        file.flush().unwrap();

        sleep_500_millis().await;
    })
    .await;

    assert_eq!(received.len(), n + 1);
}

#[tokio::test]
async fn file_truncate() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };
    let path = dir.path().join("file");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        for i in 0..n {
            writeln!(&mut file, "pretrunc {i}").unwrap();
        }

        file.flush().unwrap();
        sleep_500_millis().await; // The writes must be observed before truncating

        file.set_len(0).unwrap();
        file.seek(std::io::SeekFrom::Start(0)).unwrap();

        file.sync_all().unwrap();
        sleep_500_millis().await; // The truncate must be observed before writing again

        for i in 0..n {
            writeln!(&mut file, "posttrunc {i}").unwrap();
        }

        file.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let mut i = 0;
    let mut pre_trunc = true;

    for event in received {
        assert_eq!(
            event.as_log()["file"].to_string_lossy(),
            path.to_str().unwrap()
        );

        let line =
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();

        if pre_trunc {
            assert_eq!(line, format!("pretrunc {}", i));
        } else {
            assert_eq!(line, format!("posttrunc {}", i));
        }

        i += 1;
        if i == n {
            i = 0;
            pre_trunc = false;
        }
    }
}

#[tokio::test]
async fn file_rotate() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let archive_path = dir.path().join("file");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        for i in 0..n {
            writeln!(&mut file, "prerot {i}").unwrap();
        }

        file.flush().unwrap();
        sleep_500_millis().await; // The writes must be observed before rotating

        fs::rename(&path, archive_path).expect("could not rename");
        file.sync_all().unwrap();

        let mut file = File::create(&path).unwrap();

        file.sync_all().unwrap();
        sleep_500_millis().await; // The rotation must be observed before writing again

        for i in 0..n {
            writeln!(&mut file, "postrot {i}").unwrap();
        }

        file.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let mut i = 0;
    let mut pre_rot = true;

    for event in received {
        assert_eq!(
            event.as_log()["file"].to_string_lossy(),
            path.to_str().unwrap()
        );

        let line =
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();

        if pre_rot {
            assert_eq!(line, format!("prerot {}", i));
        } else {
            assert_eq!(line, format!("postrot {}", i));
        }

        i += 1;
        if i == n {
            i = 0;
            pre_rot = false;
        }
    }
}

#[tokio::test]
async fn file_multiple_paths() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*.txt"), dir.path().join("a.*")],
        exclude: vec![dir.path().join("a.*.txt")],
        ..test_default_file_config(&dir)
    };

    let path1 = dir.path().join("a.txt");
    let path2 = dir.path().join("b.txt");
    let path3 = dir.path().join("a.log");
    let path4 = dir.path().join("a.ignore.txt");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file1 = File::create(&path1).unwrap();
        let mut file2 = File::create(&path2).unwrap();
        let mut file3 = File::create(&path3).unwrap();
        let mut file4 = File::create(&path4).unwrap();

        for i in 0..n {
            writeln!(&mut file1, "1 {i}").unwrap();
            writeln!(&mut file2, "2 {i}").unwrap();
            writeln!(&mut file3, "3 {i}").unwrap();
            writeln!(&mut file4, "4 {i}").unwrap();
        }
        file1.flush().unwrap();
        file2.flush().unwrap();
        file3.flush().unwrap();
        file4.flush().unwrap();

        sleep_500_millis().await;
    })
    .await;

    let mut is = [0; 3];

    for event in received {
        let line =
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
        let mut split = line.split(' ');
        let file = split.next().unwrap().parse::<usize>().unwrap();
        assert_ne!(file, 4);
        let i = split.next().unwrap().parse::<usize>().unwrap();

        assert_eq!(is[file - 1], i);
        is[file - 1] += 1;
    }

    assert_eq!(is, [n as usize; 3]);
}

#[tokio::test]
async fn file_exclude_paths() {
    let n = 5;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("a//b/*.log.*")],
        exclude: vec![dir.path().join("a//b/test.log.*")],
        ..test_default_file_config(&dir)
    };

    let path1 = dir.path().join("a//b/a.log.1");
    let path2 = dir.path().join("a//b/test.log.1");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        let mut file1 = File::create(&path1).unwrap();
        let mut file2 = File::create(&path2).unwrap();

        for i in 0..n {
            writeln!(&mut file1, "1 {i}").unwrap();
            writeln!(&mut file2, "2 {i}").unwrap();
        }

        file1.flush().unwrap();
        file2.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let mut is = [0; 1];

    for event in received {
        let line =
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy();
        let mut split = line.split(' ');
        let file = split.next().unwrap().parse::<usize>().unwrap();
        assert_ne!(file, 4);
        let i = split.next().unwrap().parse::<usize>().unwrap();

        assert_eq!(is[file - 1], i);
        is[file - 1] += 1;
    }

    assert_eq!(is, [n as usize; 1]);
}

#[tokio::test]
async fn file_key_acknowledged() {
    file_key(Acks).await
}

#[tokio::test]
async fn file_key_no_acknowledge() {
    file_key(NoAcks).await
}

async fn file_key(acks: AckingMode) {
    // Default
    {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            writeln!(&mut file, "hello there").unwrap();
            file.flush().unwrap();

            sleep_500_millis().await;
        })
        .await;

        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["file"].to_string_lossy(),
            path.to_str().unwrap()
        );
    }

    // Custom
    {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            file_key: OptionalValuePath::from(owned_value_path!("source")),
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            writeln!(&mut file, "hello there").unwrap();
            file.flush().unwrap();

            sleep_500_millis().await;
        })
        .await;

        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log()["source"].to_string_lossy(),
            path.to_str().unwrap()
        );
    }

    // Hidden
    {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let received = run_file_source(&config, true, acks, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();

            writeln!(&mut file, "hello there").unwrap();

            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].as_log().keys().unwrap().collect::<HashSet<_>>(),
            vec![
                default_file_key()
                    .path
                    .expect("file key to exist")
                    .to_string()
                    .into(),
                log_schema().host_key().unwrap().to_string().into(),
                log_schema().message_key().unwrap().to_string().into(),
                log_schema().timestamp_key().unwrap().to_string().into(),
                log_schema().source_type_key().unwrap().to_string().into()
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }
}

#[tokio::test]
async fn file_start_position_server_restart_acknowledged() {
    file_start_position_server_restart(Acks).await
}

#[tokio::test]
async fn file_start_position_server_restart_no_acknowledge() {
    file_start_position_server_restart(NoAcks).await
}

async fn file_start_position_server_restart(acking: AckingMode) {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let mut file = File::create(&path).unwrap();
    writeln!(&mut file, "zeroth line").unwrap();
    file.flush().unwrap();

    // First time server runs it picks up existing lines.
    {
        let received = run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
            sleep_500_millis().await;
            writeln!(&mut file, "first line").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["zeroth line", "first line"]);
    }
    // Restart server, read file from checkpoint.
    {
        let received = run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
            sleep_500_millis().await;
            writeln!(&mut file, "second line").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["second line"]);
    }
    // Restart server, read files from beginning.
    {
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ignore_checkpoints: Some(true),
            read_from: ReadFromConfig::Beginning,
            ..test_default_file_config(&dir)
        };
        let received = run_file_source(&config, false, acking, LogNamespace::Legacy, None, async {
            sleep_500_millis().await;
            writeln!(&mut file, "third line").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(
            lines,
            vec!["zeroth line", "first line", "second line", "third line"]
        );
    }
}

#[tokio::test]
async fn file_start_position_server_restart_unfinalized() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let mut file = File::create(&path).unwrap();
    writeln!(&mut file, "the line").unwrap();
    file.flush().unwrap();

    // First time server runs it picks up existing lines.
    let received = run_file_source(
        &config,
        false,
        Unfinalized,
        LogNamespace::Legacy,
        None,
        sleep(Duration::from_secs(5)),
    )
    .await;
    let lines = extract_messages_string(received);
    assert_eq!(lines, vec!["the line"]);

    // Restart server, it re-reads file since the events were not acknowledged before shutdown
    let received = run_file_source(
        &config,
        false,
        Unfinalized,
        LogNamespace::Legacy,
        None,
        sleep(Duration::from_secs(5)),
    )
    .await;
    let lines = extract_messages_string(received);
    assert_eq!(lines, vec!["the line"]);
}

#[tokio::test]
async fn file_duplicate_processing_after_restart() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let mut file = File::create(&path).unwrap();

    let line_count = 4000;
    for i in 0..line_count {
        writeln!(&mut file, "Here's a line for you: {i}").unwrap();
    }
    file.flush().unwrap();

    // First time server runs it should pick up a bunch of lines
    let received = run_file_source(
        &config,
        true,
        Acks,
        LogNamespace::Legacy,
        None,
        // shutdown signal is sent after this duration
        sleep_500_millis(),
    )
    .await;
    let lines = extract_messages_string(received);

    // ...but not all the lines; if the first run processed the entire file, we may not hit the
    // bug we're testing for, which happens if the finalizer stream exits on shutdown with pending acks
    assert!(lines.len() < line_count);

    // Restart the server, and it should read the rest without duplicating any.
    // Use the event counter to drain rx continuously (removing backpressure so
    // the file server can read all remaining lines without being stalled), then
    // trigger shutdown once all expected events have been received.
    let remaining = line_count - lines.len();
    let event_count = Arc::new(AtomicUsize::new(0));
    let received = run_file_source(
        &config,
        true,
        Acks,
        LogNamespace::Legacy,
        Some(Arc::clone(&event_count)),
        async {
            wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= remaining, 5_000)
                .await;
        },
    )
    .await;
    let lines2 = extract_messages_string(received);

    // Between both runs, we should have the expected number of lines
    assert_eq!(lines.len() + lines2.len(), line_count);
}

#[tokio::test]
async fn file_start_position_server_restart_with_file_rotation_acknowledged() {
    file_start_position_server_restart_with_file_rotation(Acks).await
}

#[tokio::test]
async fn file_start_position_server_restart_with_file_rotation_no_acknowledge() {
    file_start_position_server_restart_with_file_rotation(NoAcks).await
}

async fn file_start_position_server_restart_with_file_rotation(acking: AckingMode) {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let path_for_old_file = dir.path().join("file.old");
    // Run server first time, collect some lines.
    {
        let received = run_file_source(&config, true, acking, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();
            writeln!(&mut file, "first line").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["first line"]);
    }
    // Perform 'file rotation' to archive old lines.
    fs::rename(&path, &path_for_old_file).expect("could not rename");
    // Restart the server and make sure it does not re-read the old file
    // even though it has a new name.
    {
        let received = run_file_source(&config, false, acking, LogNamespace::Legacy, None, async {
            let mut file = File::create(&path).unwrap();
            writeln!(&mut file, "second line").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;

        let lines = extract_messages_string(received);
        assert_eq!(lines, vec!["second line"]);
    }
}

#[cfg(unix)] // this test uses unix-specific function `futimes` during test time
#[tokio::test]
async fn file_start_position_ignore_old_files() {
    use std::{
        os::unix::io::AsRawFd,
        time::{Duration, SystemTime},
    };

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ignore_older_secs: Some(5),
        ..test_default_file_config(&dir)
    };

    let before_path = dir.path().join("before");
    let mut before_file = File::create(&before_path).unwrap();
    let after_path = dir.path().join("after");
    let mut after_file = File::create(&after_path).unwrap();

    writeln!(&mut before_file, "first line").unwrap(); // first few bytes make up unique file fingerprint
    writeln!(&mut after_file, "_first line").unwrap(); //   and therefore need to be non-identical

    {
        // Set the modified times
        let before = SystemTime::now() - Duration::from_secs(8);
        let after = SystemTime::now() - Duration::from_secs(2);

        let before_time = libc::timeval {
            tv_sec: before
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as _,
            tv_usec: 0,
        };
        let before_times = [before_time, before_time];

        let after_time = libc::timeval {
            tv_sec: after
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as _,
            tv_usec: 0,
        };
        let after_times = [after_time, after_time];

        unsafe {
            libc::futimes(before_file.as_raw_fd(), before_times.as_ptr());
            libc::futimes(after_file.as_raw_fd(), after_times.as_ptr());
        }
    }

    before_file.sync_all().unwrap();
    after_file.sync_all().unwrap();

    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        sleep_500_millis().await;
        writeln!(&mut before_file, "second line").unwrap();
        writeln!(&mut after_file, "_second line").unwrap();

        before_file.flush().unwrap();
        after_file.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let before_lines = received
        .iter()
        .filter(|event| event.as_log()["file"].to_string_lossy().ends_with("before"))
        .map(|event| {
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy()
        })
        .collect::<Vec<_>>();
    let after_lines = received
        .iter()
        .filter(|event| event.as_log()["file"].to_string_lossy().ends_with("after"))
        .map(|event| {
            event.as_log()[log_schema().message_key().unwrap().to_string()].to_string_lossy()
        })
        .collect::<Vec<_>>();
    assert_eq!(before_lines, vec!["second line"]);
    assert_eq!(after_lines, vec!["_first line", "_second line"]);
}

#[tokio::test]
async fn file_max_line_bytes() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        max_line_bytes: 10,
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "short").unwrap();
        writeln!(&mut file, "this is too long").unwrap();
        writeln!(&mut file, "11 eleven11").unwrap();
        let super_long = "This line is super long and will take up more space than BufReader's internal buffer, just to make sure that everything works properly when multiple read calls are involved".repeat(10000);
        writeln!(&mut file, "{super_long}").unwrap();
        writeln!(&mut file, "exactly 10").unwrap();
        writeln!(&mut file, "it can end on a line that's too long").unwrap();

        file.flush().unwrap();
        sleep_500_millis().await;
        sleep_500_millis().await;

        writeln!(&mut file, "and then continue").unwrap();
        writeln!(&mut file, "last short").unwrap();
        file.flush().unwrap();

        sleep_500_millis().await;
        sleep_500_millis().await;
    }).await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec!["short".into(), "exactly 10".into(), "last short".into()]
    );
}

#[tokio::test]
async fn test_multi_line_aggregation_legacy() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        message_start_indicator: Some("INFO".into()),
        multi_line_timeout: 25,
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let event_count = Arc::new(AtomicUsize::new(0));
    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        Some(Arc::clone(&event_count)),
        async {
            let mut file = File::create(&path).unwrap();

            // Write all lines through the second "INFO hello". Events 1-4
            // are emitted immediately by EndExclude; event 5 ("INFO hello"
            // standalone) requires the 25ms timeout to fire.
            writeln!(&mut file, "leftover foo").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            writeln!(&mut file, "INFO goodbye").unwrap();
            writeln!(&mut file, "part of goodbye").unwrap();
            writeln!(&mut file, "INFO hi again").unwrap();
            writeln!(&mut file, "and some more").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            file.flush().unwrap();

            // Block until event 5 is observed: the timeout fired and
            // "INFO hello" was emitted before we write "too slow".
            wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 5, 500).await;

            writeln!(&mut file, "too slow").unwrap();
            writeln!(&mut file, "INFO doesn't have").unwrap();
            writeln!(&mut file, "to be INFO in").unwrap();
            writeln!(&mut file, "the middle").unwrap();
            file.flush().unwrap();

            // Wait for events 6 ("too slow") and 7 ("INFO doesn't have")
            // before triggering shutdown.
            wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 7, 500).await;
        },
    )
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "leftover foo".into(),
            "INFO hello".into(),
            "INFO goodbye\npart of goodbye".into(),
            "INFO hi again\nand some more".into(),
            "INFO hello".into(),
            "too slow".into(),
            "INFO doesn't have".into(),
            "to be INFO in\nthe middle".into(),
        ]
    );
}

#[tokio::test]
async fn test_multi_line_aggregation() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        multiline: Some(MultilineConfig {
            start_pattern: "INFO".to_owned(),
            condition_pattern: "INFO".to_owned(),
            mode: line_agg::Mode::HaltBefore,
            timeout_ms: Duration::from_millis(25),
        }),
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let event_count = Arc::new(AtomicUsize::new(0));
    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        Some(Arc::clone(&event_count)),
        async {
            let mut file = File::create(&path).unwrap();

            // Write all lines through the second "INFO hello". Events 1-4
            // are emitted immediately by EndExclude; event 5 ("INFO hello"
            // standalone) requires the 25ms timeout to fire.
            writeln!(&mut file, "leftover foo").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            writeln!(&mut file, "INFO goodbye").unwrap();
            writeln!(&mut file, "part of goodbye").unwrap();
            writeln!(&mut file, "INFO hi again").unwrap();
            writeln!(&mut file, "and some more").unwrap();
            writeln!(&mut file, "INFO hello").unwrap();
            file.flush().unwrap();

            // Block until event 5 is observed: the timeout fired and
            // "INFO hello" was emitted before we write "too slow".
            wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 5, 500).await;

            writeln!(&mut file, "too slow").unwrap();
            writeln!(&mut file, "INFO doesn't have").unwrap();
            writeln!(&mut file, "to be INFO in").unwrap();
            writeln!(&mut file, "the middle").unwrap();
            file.flush().unwrap();

            // Wait for events 6 ("too slow") and 7 ("INFO doesn't have")
            // before triggering shutdown.
            wait_for_atomic_usize_timeout_ms(Arc::clone(&event_count), |n| n >= 7, 500).await;
        },
    )
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "leftover foo".into(),
            "INFO hello".into(),
            "INFO goodbye\npart of goodbye".into(),
            "INFO hi again\nand some more".into(),
            "INFO hello".into(),
            "too slow".into(),
            "INFO doesn't have".into(),
            "to be INFO in\nthe middle".into(),
        ]
    );
}

#[tokio::test]
async fn test_multi_line_checkpointing() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        offset_key: Some(OptionalValuePath::from(owned_value_path!("offset"))),
        multiline: Some(MultilineConfig {
            start_pattern: "INFO".to_owned(),
            condition_pattern: "INFO".to_owned(),
            mode: line_agg::Mode::HaltBefore,
            timeout_ms: Duration::from_millis(25), // less than 50 in sleep()
        }),
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let mut file = File::create(&path).unwrap();

    writeln!(&mut file, "INFO hello").unwrap();
    writeln!(&mut file, "part of hello").unwrap();

    file.sync_all().unwrap();

    // Read and aggregate existing lines. wait_shutdown=true ensures the
    // checkpoint is fully written to disk before the second run reads it.
    let received = run_file_source(
        &config,
        true,
        Acks,
        LogNamespace::Legacy,
        None,
        sleep_500_millis(),
    )
    .await;

    assert_eq!(received[0].as_log()["offset"], 0.into());

    let lines = extract_messages_string(received);
    assert_eq!(lines, vec!["INFO hello\npart of hello"]);

    // After restart, we should not see any part of the previously aggregated lines
    let received_after_restart =
        run_file_source(&config, false, Acks, LogNamespace::Legacy, None, async {
            writeln!(&mut file, "INFO goodbye").unwrap();
            file.flush().unwrap();
            sleep_500_millis().await;
        })
        .await;
    assert_eq!(
        received_after_restart[0].as_log()["offset"],
        (lines[0].len() + 1).into()
    );
    let lines = extract_messages_string(received_after_restart);
    assert_eq!(lines, vec!["INFO goodbye"]);
}

#[tokio::test]
async fn test_fair_reads() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        max_read_bytes: 1,
        oldest_first: false,
        ..test_default_file_config(&dir)
    };

    let older_path = dir.path().join("z_older_file");
    let mut older = File::create(&older_path).unwrap();

    writeln!(&mut older, "hello i am the old file").unwrap();
    writeln!(&mut older, "i have been around a while").unwrap();
    writeln!(&mut older, "you can read newer files at the same time").unwrap();
    older.sync_all().unwrap();

    let newer_path = dir.path().join("a_newer_file");
    let mut newer = File::create(&newer_path).unwrap();

    writeln!(&mut newer, "and i am the new file").unwrap();
    writeln!(&mut newer, "this should be interleaved with the old one").unwrap();
    writeln!(&mut newer, "which is fine because we want fairness").unwrap();
    newer.sync_all().unwrap();

    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        None,
        sleep_500_millis(),
    )
    .await;

    let received = extract_messages_value(received);

    let old_first = vec![
        "hello i am the old file".into(),
        "and i am the new file".into(),
        "i have been around a while".into(),
        "this should be interleaved with the old one".into(),
        "you can read newer files at the same time".into(),
        "which is fine because we want fairness".into(),
    ];
    let new_first: Vec<_> = old_first
        .chunks(2)
        .flat_map(|chunk| chunk.iter().rev().cloned().collect::<Vec<_>>())
        .collect();

    if received[0] == old_first[0] {
        assert_eq!(received, old_first);
    } else {
        assert_eq!(received, new_first);
    }
}

#[tokio::test]
async fn test_oldest_first() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        max_read_bytes: 1,
        oldest_first: true,
        ..test_default_file_config(&dir)
    };

    let older_path = dir.path().join("z_older_file");
    let mut older = File::create(&older_path).unwrap();
    older.sync_all().unwrap();

    // Sleep to ensure the creation timestamps are different
    sleep_500_millis().await;

    let newer_path = dir.path().join("a_newer_file");
    let mut newer = File::create(&newer_path).unwrap();
    newer.sync_all().unwrap();

    writeln!(&mut older, "hello i am the old file").unwrap();
    writeln!(&mut older, "i have been around a while").unwrap();
    writeln!(&mut older, "you should definitely read all of me first").unwrap();
    older.flush().unwrap();

    writeln!(&mut newer, "i'm new").unwrap();
    writeln!(&mut newer, "hopefully you read all the old stuff first").unwrap();
    writeln!(&mut newer, "because otherwise i'm not going to make sense").unwrap();
    newer.flush().unwrap();

    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        None,
        sleep_500_millis(),
    )
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "hello i am the old file".into(),
            "i have been around a while".into(),
            "you should definitely read all of me first".into(),
            "i'm new".into(),
            "hopefully you read all the old stuff first".into(),
            "because otherwise i'm not going to make sense".into(),
        ]
    );
}

#[tokio::test]
async fn test_split_reads() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        max_read_bytes: 1,
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let mut file = File::create(&path).unwrap();

    writeln!(&mut file, "hello i am a normal line").unwrap();
    file.sync_all().unwrap();

    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        sleep_500_millis().await;

        write!(&mut file, "i am not a full line").unwrap();

        file.flush().unwrap();
        // Longer than the EOF timeout
        sleep_500_millis().await;

        writeln!(&mut file, " until now").unwrap();

        file.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "hello i am a normal line".into(),
            "i am not a full line until now".into(),
        ]
    );
}

#[tokio::test]
async fn test_gzipped_file() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![PathBuf::from("tests/data/gzipped.log")],
        // TODO: remove this once files are fingerprinted after decompression
        //
        // Currently, this needs to be smaller than the total size of the compressed file
        // because the fingerprinter tries to read until a newline, which it's not going to see
        // in the compressed data, or this number of bytes. If it hits EOF before that, it
        // can't return a fingerprint because the value would change once more data is written.
        max_line_bytes: 100,
        ..test_default_file_config(&dir)
    };

    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        None,
        sleep_500_millis(),
    )
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "this is a simple file".into(),
            "i have been compressed".into(),
            "in order to make me smaller".into(),
            "but you can still read me".into(),
            "hooray".into(),
        ]
    );
}

#[tokio::test]
async fn test_non_utf8_encoded_file() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![PathBuf::from("tests/data/utf-16le.log")],
        encoding: Some(EncodingConfig { charset: UTF_16LE }),
        ..test_default_file_config(&dir)
    };

    let received = run_file_source(
        &config,
        false,
        NoAcks,
        LogNamespace::Legacy,
        None,
        sleep_500_millis(),
    )
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "hello i am a file".into(),
            "i can unicode".into(),
            "but i do so in 16 bits".into(),
            "and when i byte".into(),
            "i become little-endian".into(),
        ]
    );
}

#[tokio::test]
async fn test_non_default_line_delimiter() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        line_delimiter: "\r\n".to_string(),
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        write!(&mut file, "hello i am a line\r\n").unwrap();
        write!(&mut file, "and i am too\r\n").unwrap();
        write!(&mut file, "CRLF is how we end\r\n").unwrap();
        write!(&mut file, "please treat us well\r\n").unwrap();

        file.flush().unwrap();
        sleep_500_millis().await;
    })
    .await;

    let received = extract_messages_value(received);

    assert_eq!(
        received,
        vec![
            "hello i am a line".into(),
            "and i am too".into(),
            "CRLF is how we end".into(),
            "please treat us well".into()
        ]
    );
}

// Regression test for https://github.com/vectordotdev/vector/issues/24027
// Tests that multi-character delimiters (like \r\n) are correctly handled when
// split across buffer boundaries. Without the fix, events would be merged together.
#[tokio::test]
async fn test_multi_char_delimiter_split_across_buffer_boundary() {
    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        line_delimiter: "\r\n".to_string(),
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let received = run_file_source(&config, false, NoAcks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        sleep_500_millis().await;

        // Create data where \r\n is split at 8KB buffer boundary
        // This reproduces the exact scenario that caused data corruption:
        // - Event 1 ends with \r at byte 8191
        // - The \n appears at byte 8192 (right at the buffer boundary)
        // - Without the fix, Event 1 and Event 2 would be merged

        let buffer_size = 8192;

        // Event 1: Position \r\n to split at first boundary
        let event1_prefix = "Event 1: ";
        let padding1_len = buffer_size - event1_prefix.len() - 1; // -1 for the \r
        write!(&mut file, "{}", event1_prefix).unwrap();
        file.write_all(&vec![b'X'; padding1_len]).unwrap();
        write!(&mut file, "\r\n").unwrap(); // \r at byte 8191, \n at byte 8192

        // Event 2: Position \r\n to split at second boundary
        let event2_prefix = "Event 2: ";
        let padding2_len = buffer_size - event2_prefix.len() - 1;
        write!(&mut file, "{}", event2_prefix).unwrap();
        file.write_all(&vec![b'Y'; padding2_len]).unwrap();
        write!(&mut file, "\r\n").unwrap(); // \r at byte 16383, \n at byte 16384

        // Event 3: Normal line without boundary split
        write!(&mut file, "Event 3: Final\r\n").unwrap();

        sleep_500_millis().await;
    })
    .await;

    let messages = extract_messages_value(received);

    // The bug would cause Events 1 and 2 to be merged into a single message
    assert_eq!(
        messages.len(),
        3,
        "Should receive exactly 3 separate events (bug would merge them)"
    );

    // Verify each event is correctly separated and starts with expected prefix
    let msg0 = messages[0].to_string_lossy();
    let msg1 = messages[1].to_string_lossy();
    let msg2 = messages[2].to_string_lossy();

    assert!(
        msg0.starts_with("Event 1: "),
        "First event should start with 'Event 1: ', got: {}",
        msg0
    );
    assert!(
        msg1.starts_with("Event 2: "),
        "Second event should start with 'Event 2: ', got: {}",
        msg1
    );
    assert_eq!(msg2, "Event 3: Final");

    // Ensure no event contains embedded CR/LF (sign of incorrect merging)
    for (i, msg) in messages.iter().enumerate() {
        let msg_str = msg.to_string_lossy();
        assert!(
            !msg_str.contains('\r'),
            "Event {} should not contain embedded \\r",
            i
        );
        assert!(
            !msg_str.contains('\n'),
            "Event {} should not contain embedded \\n",
            i
        );
    }
}

#[tokio::test]
async fn remove_file() {
    let n = 5;
    let remove_after_secs = 1;

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        remove_after_secs: Some(remove_after_secs),
        ..test_default_file_config(&dir)
    };

    let path = dir.path().join("file");
    let received = run_file_source(&config, false, Acks, LogNamespace::Legacy, None, async {
        let mut file = File::create(&path).unwrap();

        for i in 0..n {
            writeln!(&mut file, "{i}").unwrap();
        }
        file.flush().unwrap();
        drop(file);

        for _ in 0..10 {
            // Wait for remove grace period to end.
            sleep(Duration::from_secs(remove_after_secs + 1)).await;

            if File::open(&path).is_err() {
                break;
            }
        }
    })
    .await;

    assert_eq!(received.len(), n);

    match File::open(&path) {
        Ok(_) => panic!("File wasn't removed"),
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AckingMode {
    NoAcks,      // No acknowledgement handling and no finalization
    Unfinalized, // Acknowledgement handling but no finalization
    Acks,        // Full acknowledgements and proper finalization
}
use AckingMode::*;
use vector_lib::lookup::OwnedTargetPath;

async fn run_file_source(
    config: &FileConfig,
    wait_shutdown: bool,
    acking_mode: AckingMode,
    log_namespace: LogNamespace,
    // When `Some`, events are relayed through an unbounded channel and the
    // counter is incremented for each event received.  The inner future can
    // call `wait_for_atomic_usize` on this counter to gate writes on
    // observed events instead of relying on wall-clock sleeps.
    event_counter: Option<Arc<AtomicUsize>>,
    inner: impl Future<Output = ()>,
) -> Vec<Event> {
    assert_source_compliance(&FILE_SOURCE_TAGS, async move {
        let (tx, rx) = match acking_mode {
            Acks => {
                let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
                (tx, rx.boxed())
            }
            Unfinalized => {
                // Use Rejected so that events are finalized but checkpoints
                // are NOT updated (only Delivered triggers checkpoint updates).
                // This avoids a race where the default Delivered status on drop
                // could leak checkpoint writes into the next run.
                let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Rejected);
                (tx, rx.boxed())
            }
            NoAcks => {
                let (tx, rx) = SourceSender::new_test();
                (tx, rx.boxed())
            }
        };

        let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
        let data_dir = config.data_dir.clone().unwrap();
        let acks = !matches!(acking_mode, NoAcks);

        tokio::spawn(file::file_source(
            config,
            data_dir,
            shutdown,
            tx,
            acks,
            log_namespace,
        ));

        let result = if let Some(counter) = event_counter {
            // Relay mode: a background task forwards events and increments
            // the counter so `inner` can observe them without arbitrary sleeps.
            let (relay_tx, mut relay_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
            tokio::spawn(async move {
                let mut rx = rx;
                while let Some(event) = rx.next().await {
                    counter.fetch_add(1, Ordering::SeqCst);
                    relay_tx.send(event).ok(); // receiver gone means pipeline is shutting down
                }
            });

            inner.await;
            drop(trigger_shutdown);

            timeout(Duration::from_secs(5), async move {
                let mut events = Vec::new();
                while let Some(event) = relay_rx.recv().await {
                    events.push(event);
                }
                events
            })
            .await
            .expect("Unclosed channel: may indicate file-server could not shutdown gracefully.")
        } else {
            inner.await;
            drop(trigger_shutdown);

            if acking_mode == Unfinalized {
                rx.take_until(tokio::time::sleep(Duration::from_secs(5)))
                    .collect::<Vec<_>>()
                    .await
            } else {
                timeout(Duration::from_secs(5), rx.collect::<Vec<_>>())
                    .await
                    .expect(
                        "Unclosed channel: may indicate file-server could not shutdown gracefully.",
                    )
            }
        };

        if wait_shutdown {
            shutdown_done.await;
        }

        result
    })
    .await
}

fn extract_messages_string(received: Vec<Event>) -> Vec<String> {
    received
        .into_iter()
        .map(Event::into_log)
        .map(|log| log.get_message().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn extract_messages_value(received: Vec<Event>) -> Vec<Value> {
    received
        .into_iter()
        .map(Event::into_log)
        .map(|log| log.get_message().unwrap().clone())
        .collect()
}
