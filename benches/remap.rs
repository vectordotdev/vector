use std::collections::HashMap;

use chrono::{DateTime, Utc};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use vector::{
    config::{DataType, TransformOutput},
    event::{Event, LogEvent, Value},
    transforms::{
        SyncTransform, TransformOutputsBuf,
        remap::{Remap, RemapConfig},
    },
};
use vrl::{event_path, prelude::*};

// ~50 realistic Datadog tags with long values and duplicate keys.
const DDTAGS_BENCH_INPUT: &str = "\
env:production,\
service:payment-gateway-service,\
version:4.12.7-rc3,\
host:ip-10-42-137-29.us-east-1.compute.internal,\
instance-type:m5.2xlarge,\
availability-zone:us-east-1c,\
region:us-east-1,\
cluster:eks-prod-main-useast1-2024,\
namespace:payments,\
pod_name:payment-gateway-service-7f8b9c6d4f-x2k9m,\
container_name:payment-gateway,\
image_tag:registry.internal.example.com/payments/gateway:4.12.7-rc3-sha-a1b2c3d4,\
team:platform-payments,\
cost_center:cc-payments-12345,\
owner:payments-oncall@example.com,\
pagerduty:payments-p1,\
slo:payments-availability-99.99,\
tier:tier-0-critical,\
compliance:pci-dss-v4,\
compliance:soc2-type2,\
compliance:gdpr,\
datacenter:us-east-1-primary,\
network:vpc-0a1b2c3d4e5f67890,\
subnet:subnet-private-us-east-1c-payments,\
security_group:sg-payment-gateway-prod,\
load_balancer:arn:aws:elasticloadbalancing:us-east-1:123456789012:loadbalancer/app/payment-gw-prod/50dc6c495c0c9188,\
target_group:arn:aws:elasticloadbalancing:us-east-1:123456789012:targetgroup/payment-gw-tg/73e2d6bc24d8a067,\
dns:payment-gateway.internal.prod.example.com,\
port:8443,\
protocol:https,\
framework:spring-boot-3.2.1,\
runtime:openjdk-21.0.2+13,\
orchestrator:kubernetes-1.29,\
deploy_pipeline:argo-cd,\
deploy_sha:a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0,\
deploy_timestamp:2024-11-15T14:32:07Z,\
canary:false,\
feature_flag:new-checkout-flow-v2,\
feature_flag:payment-retry-logic-v3,\
feature_flag:fraud-detection-ml-model-2024q4,\
circuit_breaker:downstream-bank-api,\
rate_limit_tier:premium,\
db_pool:payments-primary-rds-cluster.cluster-abc123def456.us-east-1.rds.amazonaws.com,\
cache_cluster:payments-redis-prod-001.abc123.0001.use1.cache.amazonaws.com,\
message_queue:arn:aws:sqs:us-east-1:123456789012:payment-events-prod,\
trace_sample_rate:0.15,\
log_level:info,\
custom_metric_prefix:payments.gateway,\
git_repository:github.com/example-org/payment-gateway-service,\
oncall_schedule:payments-primary-rotation-2024";

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_remap
);
criterion_main!(benches);

fn benchmark_remap(c: &mut Criterion) {
    let mut group = c.benchmark_group("remap");

    let add_fields_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all_bits(), HashMap::new())],
            1,
        );
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(
            output_1.get(event_path!("foo")).unwrap().to_string_lossy(),
            "bar"
        );
        debug_assert_eq!(
            output_1.get(event_path!("bar")).unwrap().to_string_lossy(),
            "baz"
        );
        debug_assert_eq!(
            output_1.get(event_path!("copy")).unwrap().to_string_lossy(),
            "buz"
        );

        result
    };

    group.bench_function("add_fields/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(
                        indoc! {r#".foo = "bar"
                            .bar = "baz"
                            .copy = string!(.copy_from)
                        "#}
                        .to_string(),
                    ),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("augment me"));
            event
                .as_mut_log()
                .insert(event_path!("copy_from"), "buz".to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| add_fields_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let json_parser_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all_bits(), HashMap::new())],
            1,
        );
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert_eq!(
            output_1.get(event_path!("foo")).unwrap().to_string_lossy(),
            r#"{"key": "value"}"#
        );
        debug_assert_eq!(
            output_1.get(event_path!("bar")).unwrap().to_string_lossy(),
            r#"{"key":"value"}"#
        );

        result
    };

    group.bench_function("parse_json/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(".bar = parse_json!(string!(.foo))".to_owned()),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("parse me"));
            event
                .as_mut_log()
                .insert("foo", r#"{"key": "value"}"#.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| json_parser_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    let coerce_runner =
        |tform: &mut Box<dyn SyncTransform>, event: Event, timestamp: DateTime<Utc>| {
            let mut outputs = TransformOutputsBuf::new_with_capacity(
                vec![TransformOutput::new(DataType::all_bits(), HashMap::new())],
                1,
            );
            tform.transform(event, &mut outputs);
            let result = outputs.take_primary();
            let output_1 = result.first().unwrap().as_log();

            debug_assert_eq!(
                output_1.get(event_path!("number")).unwrap(),
                &Value::Integer(1234)
            );
            debug_assert_eq!(
                output_1.get(event_path!("bool")).unwrap(),
                &Value::Boolean(true)
            );
            debug_assert_eq!(
                output_1.get(event_path!("timestamp")).unwrap(),
                &Value::Timestamp(timestamp),
            );

            result
        };

    group.bench_function("coerce/remap", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(RemapConfig {
                source: Some(indoc! {r#"
                    .number = to_int!(.number)
                    .bool = to_bool!(.bool)
                    .timestamp = parse_timestamp!(string!(.timestamp), format: "%d/%m/%Y:%H:%M:%S %z")
                "#}
                .to_owned()),
                file: None,
                timezone: None,
                drop_on_error: true,
                drop_on_abort: true,
                    ..Default::default()
            }, &Default::default())
            .unwrap()
            .0,
        );

        let mut event = Event::Log(LogEvent::from("coerce me"));
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("timestamp", "19/06/2019:17:20:49 -0400"),
        ] {
            event.as_mut_log().insert(event_path!(key), value.to_owned());
        }

        let timestamp =
            DateTime::parse_from_str("19/06/2019:17:20:49 -0400", "%d/%m/%Y:%H:%M:%S %z")
                .unwrap()
                .with_timezone(&Utc);

        b.iter_batched(
            || event.clone(),
            |event| coerce_runner(&mut tform, event, timestamp),
            BatchSize::SmallInput,
        );
    });

    let parse_ddtags_runner = |tform: &mut Box<dyn SyncTransform>, event: Event| {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![TransformOutput::new(DataType::all_bits(), HashMap::new())],
            1,
        );
        tform.transform(event, &mut outputs);
        let result = outputs.take_primary();
        let output_1 = result.first().unwrap().as_log();

        debug_assert!(output_1.get(event_path!("parsed")).is_some());

        result
    };

    group.bench_function("parse_ddtags/native", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(
                        r#".parsed = parse_ddtags!(string!(.ddtags))"#.to_string(),
                    ),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("parse ddtags"));
            event
                .as_mut_log()
                .insert(event_path!("ddtags"), DDTAGS_BENCH_INPUT.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| parse_ddtags_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("parse_ddtags/pure_vrl", |b| {
        let mut tform: Box<dyn SyncTransform> = Box::new(
            Remap::new_ast(
                RemapConfig {
                    source: Some(
                        indoc! {r#"
                            tags = split!(string!(.ddtags), ",")
                            result = {}
                            for_each(tags) -> |_i, tag| {
                                parts = split(tag, ":", limit: 2)
                                key = strip_whitespace!(to_string!(get!(parts, [0])))
                                val_raw = get(parts, [1]) ?? null
                                val = if val_raw != null {
                                    strip_whitespace!(to_string!(val_raw))
                                } else {
                                    true
                                }
                                existing = get(result, [key]) ?? null
                                if existing == null {
                                    result = set!(result, [key], [val])
                                } else {
                                    result = set!(result, [key], push!(array!(existing), val))
                                }
                            }
                            .parsed = result
                        "#}
                        .to_string(),
                    ),
                    file: None,
                    timezone: None,
                    drop_on_error: true,
                    drop_on_abort: true,
                    ..Default::default()
                },
                &Default::default(),
            )
            .unwrap()
            .0,
        );

        let event = {
            let mut event = Event::Log(LogEvent::from("parse ddtags"));
            event
                .as_mut_log()
                .insert(event_path!("ddtags"), DDTAGS_BENCH_INPUT.to_owned());
            event
        };

        b.iter_batched(
            || event.clone(),
            |event| parse_ddtags_runner(&mut tform, event),
            BatchSize::SmallInput,
        );
    });
}
