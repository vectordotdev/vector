use std::{fs, path::Path};

use tempfile::tempdir;
use tokio::time::{Duration, Instant, sleep, timeout};
use vrl::value::{Value, kind::Collection};

use super::*;
use crate::{
    config::ComponentKey,
    event::{Event, EventStatus},
    test_util::components::assert_source_compliance,
};

const TEST_COMPONENT: &str = "journald-test";
const TEST_JOURNALCTL: &str = "tests/data/journalctl";

async fn run_with_units(iunits: &[&str], xunits: &[&str], cursor: Option<&str>) -> Vec<Event> {
    let include_matches = create_unit_matches(iunits.to_vec());
    let exclude_matches = create_unit_matches(xunits.to_vec());
    run_journal(include_matches, exclude_matches, cursor, false).await
}

async fn run_journal(
    include_matches: Matches,
    exclude_matches: Matches,
    checkpoint: Option<&str>,
    emit_cursor: bool,
) -> Vec<Event> {
    assert_source_compliance(&["protocol"], async move {
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let tempdir = tempdir().unwrap();
        let tempdir = tempdir.path().to_path_buf();

        if let Some(cursor) = checkpoint {
            let mut checkpoint_path = tempdir.clone();
            checkpoint_path.push(TEST_COMPONENT);
            fs::create_dir(&checkpoint_path).unwrap();
            checkpoint_path.push(CHECKPOINT_FILENAME);

            let mut checkpointer = Checkpointer::new(checkpoint_path.clone())
                .await
                .expect("Creating checkpointer failed!");

            checkpointer
                .set(cursor)
                .await
                .expect("Could not set checkpoint");
        }

        let (cx, shutdown) = SourceContext::new_shutdown(&ComponentKey::from(TEST_COMPONENT), tx);
        let config = JournaldConfig {
            journalctl_path: Some(TEST_JOURNALCTL.into()),
            include_matches,
            exclude_matches,
            data_dir: Some(tempdir),
            remap_priority: true,
            acknowledgements: false.into(),
            emit_cursor,
            ..Default::default()
        };
        let source = config.build(cx).await.unwrap();
        tokio::spawn(async move { source.await.unwrap() });

        // Hack: Sleep to ensure journalctl process starts and emits events before shutdown.
        sleep(Duration::from_secs(1)).await;
        shutdown
            .shutdown_all(Some(Instant::now() + Duration::from_secs(1)))
            .await;

        timeout(Duration::from_secs(1), rx.collect()).await.unwrap()
    })
    .await
}

fn create_unit_matches<S: Into<String>>(units: Vec<S>) -> Matches {
    let units: HashSet<String> = units.into_iter().map(Into::into).collect();
    let mut map = HashMap::new();
    if !units.is_empty() {
        map.insert(String::from(SYSTEMD_UNIT), units);
    }
    map
}

fn create_matches<S: Into<String>>(conditions: Vec<(S, S)>) -> Matches {
    let mut matches: Matches = HashMap::new();
    for (field, value) in conditions {
        matches
            .entry(field.into())
            .or_default()
            .insert(value.into());
    }
    matches
}

#[tokio::test]
async fn reads_journal() {
    let received = run_with_units(&[], &[], None).await;
    assert_eq!(received.len(), 8);
    assert_eq!(
        message(&received[0]),
        Value::Bytes("System Initialization".into())
    );
    assert_eq!(
        received[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
        "journald".into()
    );
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140001000));
    assert_eq!(priority(&received[0]), Value::Bytes("INFO".into()));
    assert_eq!(message(&received[1]), Value::Bytes("unit message".into()));
    assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140002000));
    assert_eq!(priority(&received[1]), Value::Bytes("DEBUG".into()));
}

#[tokio::test]
async fn includes_units() {
    let received = run_with_units(&["unit.service"], &[], None).await;
    assert_eq!(received.len(), 1);
    assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
}

#[tokio::test]
async fn excludes_units() {
    let received = run_with_units(&[], &["unit.service", "badunit.service"], None).await;
    assert_eq!(received.len(), 6);
    assert_eq!(
        message(&received[0]),
        Value::Bytes("System Initialization".into())
    );
    assert_eq!(
        message(&received[1]),
        Value::Bytes("Missing timestamp".into())
    );
    assert_eq!(
        message(&received[2]),
        Value::Bytes("Different timestamps".into())
    );
}

#[tokio::test]
async fn emits_cursor() {
    let received = run_journal(Matches::new(), Matches::new(), None, true).await;
    assert_eq!(cursor(&received[0]), Value::Bytes("1".into()));
    assert_eq!(cursor(&received[3]), Value::Bytes("4".into()));
    assert_eq!(cursor(&received[7]), Value::Bytes("8".into()));
}

#[tokio::test]
async fn includes_matches() {
    let matches = create_matches(vec![("PRIORITY", "ERR")]);
    let received = run_journal(matches, HashMap::new(), None, false).await;
    assert_eq!(received.len(), 2);
    assert_eq!(
        message(&received[0]),
        Value::Bytes("Different timestamps".into())
    );
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140005000));
    assert_eq!(
        message(&received[1]),
        Value::Bytes("Non-ASCII in other field".into())
    );
    assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140005000));
}

#[tokio::test]
async fn includes_kernel() {
    let matches = create_matches(vec![("_TRANSPORT", "kernel")]);
    let received = run_journal(matches, HashMap::new(), None, false).await;
    assert_eq!(received.len(), 1);
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140006000));
    assert_eq!(message(&received[0]), Value::Bytes("audit log".into()));
}

#[tokio::test]
async fn excludes_matches() {
    let matches = create_matches(vec![("PRIORITY", "INFO"), ("PRIORITY", "DEBUG")]);
    let received = run_journal(HashMap::new(), matches, None, false).await;
    assert_eq!(received.len(), 5);
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140003000));
    assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140004000));
    assert_eq!(timestamp(&received[2]), value_ts(1578529839, 140005000));
    assert_eq!(timestamp(&received[3]), value_ts(1578529839, 140005000));
    assert_eq!(timestamp(&received[4]), value_ts(1578529839, 140006000));
}

#[tokio::test]
async fn handles_checkpoint() {
    let received = run_with_units(&[], &[], Some("1")).await;
    assert_eq!(received.len(), 7);
    assert_eq!(message(&received[0]), Value::Bytes("unit message".into()));
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140002000));
}

#[tokio::test]
async fn parses_array_messages() {
    let received = run_with_units(&["badunit.service"], &[], None).await;
    assert_eq!(received.len(), 1);
    assert_eq!(message(&received[0]), Value::Bytes("¿Hello?".into()));
}

#[tokio::test]
async fn parses_array_fields() {
    let received = run_with_units(&["syslog.service"], &[], None).await;
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].as_log()["SYSLOG_RAW"],
        Value::Bytes("¿World?".into())
    );
}

#[tokio::test]
async fn parses_string_sequences() {
    let received = run_with_units(&["NetworkManager.service"], &[], None).await;
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].as_log()["SYSLOG_FACILITY"],
        Value::Bytes(r#"["DHCP4","DHCP6"]"#.into())
    );
}

#[tokio::test]
async fn handles_missing_timestamp() {
    let received = run_with_units(&["stdout"], &[], None).await;
    assert_eq!(received.len(), 2);
    assert_eq!(timestamp(&received[0]), value_ts(1578529839, 140004000));
    assert_eq!(timestamp(&received[1]), value_ts(1578529839, 140005000));
}

#[tokio::test]
async fn handles_acknowledgements() {
    let (tx, mut rx) = SourceSender::new_test();

    let tempdir = tempdir().unwrap();
    let tempdir = tempdir.path().to_path_buf();
    let mut checkpoint_path = tempdir.clone();
    checkpoint_path.push(TEST_COMPONENT);
    fs::create_dir(&checkpoint_path).unwrap();
    checkpoint_path.push(CHECKPOINT_FILENAME);

    let mut checkpointer = Checkpointer::new(checkpoint_path.clone())
        .await
        .expect("Creating checkpointer failed!");

    let config = JournaldConfig {
        journalctl_path: Some(TEST_JOURNALCTL.into()),
        data_dir: Some(tempdir),
        remap_priority: true,
        acknowledgements: true.into(),
        ..Default::default()
    };
    let (cx, _shutdown) = SourceContext::new_shutdown(&ComponentKey::from(TEST_COMPONENT), tx);
    let source = config.build(cx).await.unwrap();
    tokio::spawn(async move { source.await.unwrap() });

    // Make sure the checkpointer cursor is empty
    assert_eq!(checkpointer.get().await.unwrap(), None);

    // Hack: Sleep to ensure journalctl process starts and emits events.
    sleep(Duration::from_secs(1)).await;

    // Acknowledge all the received events.
    let mut count = 0;
    while let Poll::Ready(Some(event)) = futures::poll!(rx.next()) {
        // The checkpointer shouldn't set the cursor until the end of the batch.
        assert_eq!(checkpointer.get().await.unwrap(), None);
        event.metadata().update_status(EventStatus::Delivered);
        count += 1;
    }
    assert_eq!(count, 8);

    sleep(Duration::from_millis(100)).await;
    assert_eq!(checkpointer.get().await.unwrap().as_deref(), Some("8"));
}

#[test]
fn filter_matches_works_correctly() {
    let empty: Matches = HashMap::new();
    let includes = create_unit_matches(vec!["one", "two"]);
    let excludes = create_unit_matches(vec!["foo", "bar"]);

    let zero = HashMap::new();
    assert!(!filter_matches(&zero, &empty, &empty));
    assert!(filter_matches(&zero, &includes, &empty));
    assert!(!filter_matches(&zero, &empty, &excludes));
    assert!(filter_matches(&zero, &includes, &excludes));
    let mut one = HashMap::new();
    one.insert(String::from(SYSTEMD_UNIT), String::from("one"));
    assert!(!filter_matches(&one, &empty, &empty));
    assert!(!filter_matches(&one, &includes, &empty));
    assert!(!filter_matches(&one, &empty, &excludes));
    assert!(!filter_matches(&one, &includes, &excludes));
    let mut two = HashMap::new();
    two.insert(String::from(SYSTEMD_UNIT), String::from("bar"));
    assert!(!filter_matches(&two, &empty, &empty));
    assert!(filter_matches(&two, &includes, &empty));
    assert!(filter_matches(&two, &empty, &excludes));
    assert!(filter_matches(&two, &includes, &excludes));
}

#[test]
fn merges_units_and_matches_option() {
    let include_units = vec!["one", "two"].into_iter().map(String::from).collect();
    let include_matches = create_matches(vec![
        ("_SYSTEMD_UNIT", "three.service"),
        ("_TRANSPORT", "kernel"),
    ]);

    let exclude_units = vec!["foo", "bar"].into_iter().map(String::from).collect();
    let exclude_matches = create_matches(vec![
        ("_SYSTEMD_UNIT", "baz.service"),
        ("PRIORITY", "DEBUG"),
    ]);

    let journald_config = JournaldConfig {
        include_units,
        include_matches,
        exclude_units,
        exclude_matches,
        ..Default::default()
    };

    let hashset = |v: &[&str]| -> HashSet<String> { v.iter().copied().map(String::from).collect() };

    let matches = journald_config.merged_include_matches();
    let units = matches.get("_SYSTEMD_UNIT").unwrap();
    assert_eq!(
        units,
        &hashset(&["one.service", "two.service", "three.service"])
    );
    let units = matches.get("_TRANSPORT").unwrap();
    assert_eq!(units, &hashset(&["kernel"]));

    let matches = journald_config.merged_exclude_matches();
    let units = matches.get("_SYSTEMD_UNIT").unwrap();
    assert_eq!(
        units,
        &hashset(&["foo.service", "bar.service", "baz.service"])
    );
    let units = matches.get("PRIORITY").unwrap();
    assert_eq!(units, &hashset(&["DEBUG"]));
}

#[test]
fn find_duplicate_match_works_correctly() {
    let include_matches = create_matches(vec![("_TRANSPORT", "kernel")]);
    let exclude_matches = create_matches(vec![("_TRANSPORT", "kernel")]);
    let (field, value) = find_duplicate_match(&include_matches, &exclude_matches).unwrap();
    assert_eq!(field, "_TRANSPORT");
    assert_eq!(value, "kernel");

    let empty = HashMap::new();
    let actual = find_duplicate_match(&empty, &empty);
    assert!(actual.is_none());

    let actual = find_duplicate_match(&include_matches, &empty);
    assert!(actual.is_none());

    let actual = find_duplicate_match(&empty, &exclude_matches);
    assert!(actual.is_none());
}

#[test]
fn command_options() {
    let path = PathBuf::from("journalctl");

    let systemd_version = 239;
    let journal_dir = None;
    let journal_namespace = None;
    let current_boot_only = false;
    let cursor = None;
    let since_now = false;
    let extra_args = vec![];

    let command = create_command(
        &path,
        systemd_version,
        journal_dir,
        journal_namespace,
        current_boot_only,
        since_now,
        cursor,
        extra_args,
    );
    let cmd_line = format!("{command:?}");
    assert!(!cmd_line.contains("--directory="));
    assert!(!cmd_line.contains("--namespace="));
    assert!(!cmd_line.contains("--boot=all"));
    assert!(cmd_line.contains("--since=2000-01-01"));

    let journal_dir = None;
    let journal_namespace = None;
    let since_now = true;
    let extra_args = vec![];

    let command = create_command(
        &path,
        systemd_version,
        journal_dir,
        journal_namespace,
        current_boot_only,
        since_now,
        cursor,
        extra_args,
    );
    let cmd_line = format!("{command:?}");
    assert!(cmd_line.contains("--since=now"));

    let journal_dir = Some(PathBuf::from("/tmp/journal-dir"));
    let journal_namespace = Some(String::from("my_namespace"));
    let current_boot_only = true;
    let cursor = Some("2021-01-01");
    let extra_args = vec!["--merge".to_string()];

    let command = create_command(
        &path,
        systemd_version,
        journal_dir,
        journal_namespace,
        current_boot_only,
        since_now,
        cursor,
        extra_args,
    );
    let cmd_line = format!("{command:?}");
    assert!(cmd_line.contains("--directory=/tmp/journal-dir"));
    assert!(cmd_line.contains("--namespace=my_namespace"));
    assert!(cmd_line.contains("--boot"));
    assert!(cmd_line.contains("--after-cursor="));
    assert!(cmd_line.contains("--merge"));

    let systemd_version = 258;
    let journal_dir = None;
    let journal_namespace = None;
    let current_boot_only = false;
    let extra_args = vec![];

    let command = create_command(
        &path,
        systemd_version,
        journal_dir,
        journal_namespace,
        current_boot_only,
        since_now,
        cursor,
        extra_args,
    );
    let cmd_line = format!("{command:?}");
    assert!(cmd_line.contains("--boot=all"));
}

#[allow(clippy::too_many_arguments)]
fn create_command(
    path: &Path,
    systemd_version: u32,
    journal_dir: Option<PathBuf>,
    journal_namespace: Option<String>,
    current_boot_only: bool,
    since_now: bool,
    cursor: Option<&str>,
    extra_args: Vec<String>,
) -> Command {
    StartJournalctl::new(
        path.into(),
        systemd_version,
        journal_dir,
        journal_namespace,
        current_boot_only,
        since_now,
        extra_args,
    )
    .make_command(cursor)
}

fn message(event: &Event) -> Value {
    event.as_log()[log_schema().message_key().unwrap().to_string()].clone()
}

fn timestamp(event: &Event) -> Value {
    event.as_log()[log_schema().timestamp_key().unwrap().to_string()].clone()
}

fn cursor(event: &Event) -> Value {
    event.as_log()[CURSOR].clone()
}

fn value_ts(secs: i64, usecs: u32) -> Value {
    Value::Timestamp(
        chrono::Utc
            .timestamp_opt(secs, usecs)
            .single()
            .expect("invalid timestamp"),
    )
}

fn priority(event: &Event) -> Value {
    event.as_log()["PRIORITY"].clone()
}

#[test]
fn output_schema_definition_vector_namespace() {
    let config = JournaldConfig {
        log_namespace: Some(true),
        ..Default::default()
    };

    let definitions = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    let expected_definition =
        Definition::new_with_default_metadata(Kind::bytes().or_null(), [LogNamespace::Vector])
            .with_metadata_field(
                &owned_value_path!("vector", "source_type"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("vector", "ingest_timestamp"),
                Kind::timestamp(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(JournaldConfig::NAME, "metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(JournaldConfig::NAME, "timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_metadata_field(
                &owned_value_path!(JournaldConfig::NAME, "host"),
                Kind::bytes().or_undefined(),
                Some("host"),
            );

    assert_eq!(definitions, Some(expected_definition))
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let config = JournaldConfig::default();

    let definitions = config
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()),
        [LogNamespace::Legacy],
    )
    .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
    .with_event_field(
        &owned_value_path!("host"),
        Kind::bytes().or_undefined(),
        Some("host"),
    )
    .unknown_fields(Kind::bytes());

    assert_eq!(definitions, Some(expected_definition))
}

fn matches_schema(config: &JournaldConfig, namespace: LogNamespace) {
    let record = r#"{
        "PRIORITY":"6",
        "SYSLOG_FACILITY":"3",
        "SYSLOG_IDENTIFIER":"ntpd",
        "_BOOT_ID":"124c781146e841ae8d9b4590df8b9231",
        "_CAP_EFFECTIVE":"3fffffffff",
        "_CMDLINE":"ntpd: [priv]",
        "_COMM":"ntpd",
        "_EXE":"/usr/sbin/ntpd",
        "_GID":"0",
        "_MACHINE_ID":"c36e9ea52800a19d214cb71b53263a28",
        "_PID":"2156",
        "_STREAM_ID":"92c79f4b45c4457490ebdefece29995e",
        "_SYSTEMD_CGROUP":"/system.slice/ntpd.service",
        "_SYSTEMD_INVOCATION_ID":"496ad5cd046d48e29f37f559a6d176f8",
        "_SYSTEMD_SLICE":"system.slice",
        "_SYSTEMD_UNIT":"ntpd.service",
        "_TRANSPORT":"stdout",
        "_UID":"0",
        "__MONOTONIC_TIMESTAMP":"98694000446",
        "__REALTIME_TIMESTAMP":"1564173027000443",
        "host":"my-host.local",
        "message":"reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s",
        "source_type":"journald"
    }"#;

    let json: serde_json::Value = serde_json::from_str(record).unwrap();
    let mut event = Event::from(LogEvent::from(vrl::value::Value::from(json)));

    event
        .as_mut_log()
        .insert(event_path!("timestamp"), chrono::Utc::now());

    let definitions = config.outputs(namespace).remove(0).schema_definition(true);

    definitions.unwrap().assert_valid_for_event(&event);
}

#[test]
fn matches_schema_legacy() {
    let config = JournaldConfig::default();

    matches_schema(&config, LogNamespace::Legacy)
}
