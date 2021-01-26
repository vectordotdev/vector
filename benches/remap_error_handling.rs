use criterion::{criterion_group, criterion_main, Criterion};
use vector::test_util::benchmark_configs;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_remap_error_handling
);

criterion_main!(benches);

fn benchmark_remap_error_handling(c: &mut Criterion) {
    let configs: Vec<(&str, &str)> = vec![
        (
            "remap_abort_on_error",
            r#"
[transforms.last]
  type = "remap"
  inputs = ["in"]
  source = """
  . = parse_syslog!(.message)
  """
         "#,
        ),
        (
            "remap_capture_error",
            r#"
[transforms.last]
  type = "remap"
  inputs = ["in"]
  source = """
  log, err = parse_syslog(.message)
  if err == null {
      . = log
  }
  """
         "#,
        ),
        (
            "remap_coalesce_error",
            r#"
[transforms.last]
  type = "remap"
  inputs = ["in"]
  source = """
  . = parse_syslog(.message) ?? {"message":"none"}
  """
         "#,
        ),
    ];

    let input = r#"<12>3 2020-12-19T21:48:09.004Z initech.io su 4015 ID81 - TPS report missing cover sheet"#;
    let output = serde_json::from_str(r#"{ "appname": "su", "facility": "user", "hostname": "initech.io", "severity": "warning", "message": "TPS report missing cover sheet", "msgid": "ID81", "procid": 4015, "timestamp": "2020-12-19 21:48:09.004 +00:00", "version": 3 }"#).unwrap();

    benchmark_configs(
        c,
        "remap_error_handling",
        configs,
        "in",
        "last",
        input,
        output,
    );
}
