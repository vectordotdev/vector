use approx::assert_relative_eq;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vrl::owned_value_path;

use crate::{
    conditions::{Condition, ConditionalConfig, VrlConfig},
    config::log_schema,
    event::{Event, LogEvent, TraceEvent},
    template::Template,
    test_util::{components::assert_transform_compliance, random_lines},
    transforms::{
        FunctionTransform, OutputBuffer,
        sample::{
            config::{SampleConfig, default_sample_rate_key},
            transform::{DynamicSampleFields, Sample, SampleMode},
        },
        test::{create_topology, transform_one},
    },
};

#[tokio::test]
async fn emits_internal_events() {
    assert_transform_compliance(async move {
        let config = SampleConfig {
            rate: None,
            ratio: Some(1.0),
            ratio_field: None,
            rate_field: None,
            key_field: None,
            group_by: None,
            exclude: None,
            sample_rate_key: default_sample_rate_key(),
        };
        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        let log = LogEvent::from("hello world");
        tx.send(log.into()).await.unwrap();

        _ = out.recv().await;

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await
}

#[test]
fn hash_samples_at_roughly_the_configured_rate() {
    let num_events = 10000;

    let events = random_events(num_events);
    let mut sampler = Sample::new(
        "sample".to_string(),
        SampleMode::new_rate(2),
        log_schema().message_key().map(ToString::to_string),
        None,
        Some(condition_contains(
            log_schema().message_key().unwrap().to_string().as_str(),
            "na",
        )),
        default_sample_rate_key(),
    );
    let total_passed = events
        .into_iter()
        .filter_map(|event| {
            let mut buf = OutputBuffer::with_capacity(1);
            sampler.transform(&mut buf, event);
            buf.into_events().next()
        })
        .count();
    let actual = total_passed as f64 / num_events as f64;
    assert_relative_eq!(sampler.ratio(), actual, epsilon = 0.03);

    let events = random_events(num_events);
    let mut sampler = Sample::new(
        "sample".to_string(),
        SampleMode::new_ratio(0.04),
        log_schema().message_key().map(ToString::to_string),
        None,
        Some(condition_contains(
            log_schema().message_key().unwrap().to_string().as_str(),
            "na",
        )),
        default_sample_rate_key(),
    );
    let total_passed = events
        .into_iter()
        .filter_map(|event| {
            let mut buf = OutputBuffer::with_capacity(1);
            sampler.transform(&mut buf, event);
            buf.into_events().next()
        })
        .count();
    let actual = total_passed as f64 / num_events as f64;
    assert_relative_eq!(sampler.ratio(), actual, epsilon = 0.03);
}

#[test]
fn hash_consistently_samples_the_same_events() {
    let events = random_events(1000);
    let mut sampler = Sample::new(
        "sample".to_string(),
        SampleMode::new_rate(2),
        log_schema().message_key().map(ToString::to_string),
        None,
        Some(condition_contains(
            log_schema().message_key().unwrap().to_string().as_str(),
            "na",
        )),
        default_sample_rate_key(),
    );

    let first_run = events
        .clone()
        .into_iter()
        .filter_map(|event| {
            let mut buf = OutputBuffer::with_capacity(1);
            sampler.transform(&mut buf, event);
            buf.into_events().next()
        })
        .collect::<Vec<_>>();
    let second_run = events
        .into_iter()
        .filter_map(|event| {
            let mut buf = OutputBuffer::with_capacity(1);
            sampler.transform(&mut buf, event);
            buf.into_events().next()
        })
        .collect::<Vec<_>>();

    assert_eq!(first_run, second_run);
}

#[test]
fn always_passes_events_matching_pass_list() {
    for key_field in &[None, log_schema().message_key().map(ToString::to_string)] {
        let event = Event::Log(LogEvent::from("i am important"));
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_rate(0),
            key_field.clone(),
            None,
            Some(condition_contains(
                log_schema().message_key().unwrap().to_string().as_str(),
                "important",
            )),
            default_sample_rate_key(),
        );
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| {
                transform_one(&mut sampler, event.clone()).map(|result| assert_eq!(result, event))
            })
            .count();
        assert_eq!(total_passed, 1000);
    }
}

#[test]
fn handles_group_by() {
    for group_by in &[None, Some(Template::try_from("{{ other_field }}").unwrap())] {
        let mut event = Event::Log(LogEvent::from("nananana"));
        let log = event.as_mut_log();
        log.insert("other_field", "foo");
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_rate(0),
            log_schema().message_key().map(ToString::to_string),
            group_by.clone(),
            Some(condition_contains(
                log_schema().message_key().unwrap().to_string().as_str(),
                "na",
            )),
            default_sample_rate_key(),
        );
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| {
                transform_one(&mut sampler, event.clone()).map(|result| assert_eq!(result, event))
            })
            .count();
        assert_eq!(total_passed, 1000);
    }
}

#[test]
fn handles_key_field() {
    for key_field in &[None, Some("other_field".into())] {
        let mut event = Event::Log(LogEvent::from("nananana"));
        let log = event.as_mut_log();
        log.insert("other_field", "foo");
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_ratio(0.0),
            key_field.clone(),
            None,
            Some(condition_contains("other_field", "foo")),
            default_sample_rate_key(),
        );
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| {
                transform_one(&mut sampler, event.clone()).map(|result| assert_eq!(result, event))
            })
            .count();
        assert_eq!(total_passed, 1000);
    }
}

#[test]
fn sampler_adds_sampling_rate_to_event() {
    for key_field in &[None, log_schema().message_key().map(ToString::to_string)] {
        let events = random_events(10000);
        let message_key = log_schema().message_key().unwrap().to_string();
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_ratio(0.1),
            key_field.clone(),
            None,
            Some(condition_contains(&message_key, "na")),
            default_sample_rate_key(),
        );
        let passing = events
            .into_iter()
            .filter(|s| !s.as_log()[&message_key].to_string_lossy().contains("na"))
            .find_map(|event| transform_one(&mut sampler, event))
            .unwrap();
        assert_eq!(passing.as_log()["sample_rate"], "0.1".into());

        let events = random_events(10000);
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_rate(25),
            key_field.clone(),
            None,
            Some(condition_contains(&message_key, "na")),
            OptionalValuePath::from(owned_value_path!("custom_sample_rate")),
        );
        let passing = events
            .into_iter()
            .filter(|s| !s.as_log()[&message_key].to_string_lossy().contains("na"))
            .find_map(|event| transform_one(&mut sampler, event))
            .unwrap();
        assert_eq!(passing.as_log()["custom_sample_rate"], "25".into());
        assert!(passing.as_log().get("sample_rate").is_none());

        let events = random_events(10000);
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_rate(2),
            key_field.clone(),
            None,
            Some(condition_contains(&message_key, "na")),
            OptionalValuePath::from(owned_value_path!("")),
        );
        let passing = events
            .into_iter()
            .filter(|s| !s.as_log()[&message_key].to_string_lossy().contains("na"))
            .find_map(|event| transform_one(&mut sampler, event))
            .unwrap();
        assert!(passing.as_log().get("sample_rate").is_none());

        // If the event passed the regex check, don't include the sampling rate
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_ratio(0.04),
            key_field.clone(),
            None,
            Some(condition_contains(&message_key, "na")),
            default_sample_rate_key(),
        );
        let event = Event::Log(LogEvent::from("nananana"));
        let passing = transform_one(&mut sampler, event).unwrap();
        assert!(passing.as_log().get("sample_rate").is_none());
    }
}

#[test]
fn handles_trace_event() {
    let event: TraceEvent = LogEvent::from("trace").into();
    let trace = Event::Trace(event);

    let mut sampler = Sample::new(
        "sample".to_string(),
        SampleMode::new_rate(2),
        None,
        None,
        None,
        default_sample_rate_key(),
    );

    let iterations = 0..2;
    let total_passed = iterations
        .filter_map(|_| transform_one(&mut sampler, trace.clone()))
        .count();
    assert_eq!(total_passed, 1);
}

#[test]
fn sample_at_rates_higher_then_half() {
    // Retain 80% of the events of the stream
    let events = random_events(10000);
    let ratios = vec![0.8, 0.7, 0.9, 0.672];
    for ratio in ratios {
        let mut sampler = Sample::new(
            "sample".to_string(),
            SampleMode::new_ratio(ratio),
            None,
            None,
            None,
            default_sample_rate_key(),
        );
        let total_observed = events
            .iter()
            .filter_map(|event| {
                let mut buf = OutputBuffer::with_capacity(1);
                sampler.transform(&mut buf, event.clone());
                buf.into_events().next()
            })
            .count();
        assert_eq!(total_observed as f64, 10000.0 * sampler.ratio());
    }
}

#[test]
fn dynamic_ratio_field_overrides_static_ratio() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.1),
        DynamicSampleFields {
            ratio_field: Some("dynamic_ratio".to_string()),
            rate_field: None,
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let mut event = Event::Log(LogEvent::from("hello"));
    let log = event.as_mut_log();
    log.insert("dynamic_ratio", 1.0);

    let output = transform_one(&mut sampler, event).expect("event should be sampled");
    assert_eq!(output.as_log()["sample_rate"], "1".into());
}

#[test]
fn dynamic_ratio_field_falls_back_to_static_ratio_when_missing() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(1.0),
        DynamicSampleFields {
            ratio_field: Some("dynamic_ratio".to_string()),
            rate_field: None,
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let event = Event::Log(LogEvent::from("hello"));
    let output = transform_one(&mut sampler, event).expect("event should be sampled");
    assert_eq!(output.as_log()["sample_rate"], "1".into());
}

#[test]
fn dynamic_ratio_field_falls_back_to_static_rate_when_missing() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_rate(2),
        DynamicSampleFields {
            ratio_field: Some("dynamic_ratio".to_string()),
            rate_field: None,
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let event = Event::Log(LogEvent::from("hello"));
    assert!(transform_one(&mut sampler, event).is_some());
}

#[test]
fn dynamic_rate_field_overrides_static_ratio() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.0),
        DynamicSampleFields {
            ratio_field: None,
            rate_field: Some("dynamic_rate".to_string()),
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let mut event = Event::Log(LogEvent::from("hello"));
    let log = event.as_mut_log();
    log.insert("dynamic_rate", 1);

    let output = transform_one(&mut sampler, event).expect("event should be sampled");
    assert_eq!(output.as_log()["sample_rate"], "1".into());
}

#[test]
fn dynamic_rate_field_falls_back_to_static_ratio_when_missing() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(1.0),
        DynamicSampleFields {
            ratio_field: None,
            rate_field: Some("dynamic_rate".to_string()),
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let event = Event::Log(LogEvent::from("hello"));
    let output = transform_one(&mut sampler, event).expect("event should be sampled");
    assert_eq!(output.as_log()["sample_rate"], "1".into());
}

#[test]
fn dynamic_rate_field_rejects_float_and_falls_back_to_static_ratio() {
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(1.0),
        DynamicSampleFields {
            ratio_field: None,
            rate_field: Some("dynamic_rate".to_string()),
        },
        None,
        None,
        default_sample_rate_key(),
    );

    let mut event = Event::Log(LogEvent::from("hello"));
    let log = event.as_mut_log();
    log.insert("dynamic_rate", 2.0);

    let output = transform_one(&mut sampler, event).expect("event should be sampled");
    assert_eq!(output.as_log()["sample_rate"], "1".into());
}

#[test]
fn dynamic_ratio_honors_group_by_key() {
    let ratio = 0.5_f64;
    let events_per_service = 200;
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.0),
        DynamicSampleFields {
            ratio_field: Some("dynamic_ratio".to_string()),
            rate_field: None,
        },
        Some(Template::try_from("{{ service }}").unwrap()),
        None,
        default_sample_rate_key(),
    );

    let mut sampled_service_a = 0;
    let mut sampled_service_b = 0;
    for _ in 0..events_per_service {
        for service in ["service-a", "service-b"] {
            let mut event = Event::Log(LogEvent::from("hello"));
            let log = event.as_mut_log();
            log.insert("service", service);
            log.insert("dynamic_ratio", ratio);
            if let Some(output) = transform_one(&mut sampler, event) {
                assert_eq!(output.as_log()["sample_rate"], "0.5".into());
                if service == "service-a" {
                    sampled_service_a += 1;
                } else {
                    sampled_service_b += 1;
                }
            }
        }
    }

    assert!(
        (60..140).contains(&sampled_service_a),
        "service-a sampled {} out of {events_per_service}",
        sampled_service_a
    );
    assert!(
        (60..140).contains(&sampled_service_b),
        "service-b sampled {} out of {events_per_service}",
        sampled_service_b
    );
}

#[test]
fn dynamic_rate_honors_group_by_key() {
    let rate = 2_i64;
    let events_per_service = 200;
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.0),
        DynamicSampleFields {
            ratio_field: None,
            rate_field: Some("dynamic_rate".to_string()),
        },
        Some(Template::try_from("{{ service }}").unwrap()),
        None,
        default_sample_rate_key(),
    );

    let mut sampled_service_a = 0;
    let mut sampled_service_b = 0;
    for _ in 0..events_per_service {
        for service in ["service-a", "service-b"] {
            let mut event = Event::Log(LogEvent::from("hello"));
            let log = event.as_mut_log();
            log.insert("service", service);
            log.insert("dynamic_rate", rate);
            if let Some(output) = transform_one(&mut sampler, event) {
                assert_eq!(output.as_log()["sample_rate"], "2".into());
                if service == "service-a" {
                    sampled_service_a += 1;
                } else {
                    sampled_service_b += 1;
                }
            }
        }
    }

    assert!(
        (60..140).contains(&sampled_service_a),
        "service-a sampled {} out of {events_per_service}",
        sampled_service_a
    );
    assert!(
        (60..140).contains(&sampled_service_b),
        "service-b sampled {} out of {events_per_service}",
        sampled_service_b
    );
}

#[test]
fn dynamic_ratio_group_by_partitions_state_by_ratio_value() {
    let events_per_ratio = 500;
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.0),
        DynamicSampleFields {
            ratio_field: Some("dynamic_ratio".to_string()),
            rate_field: None,
        },
        Some(Template::try_from("{{ service }}").unwrap()),
        None,
        default_sample_rate_key(),
    );

    let mut sampled_low_ratio = 0;
    let mut sampled_high_ratio = 0;
    for _ in 0..events_per_ratio {
        for (ratio, is_low_ratio) in [(0.25_f64, true), (0.75_f64, false)] {
            let mut event = Event::Log(LogEvent::from("hello"));
            let log = event.as_mut_log();
            log.insert("service", "service-a");
            log.insert("dynamic_ratio", ratio);
            if let Some(output) = transform_one(&mut sampler, event) {
                assert_eq!(output.as_log()["sample_rate"], ratio.to_string().into());
                if is_low_ratio {
                    sampled_low_ratio += 1;
                } else {
                    sampled_high_ratio += 1;
                }
            }
        }
    }

    assert!(
        (80..220).contains(&sampled_low_ratio),
        "ratio=0.25 sampled {} out of {events_per_ratio}",
        sampled_low_ratio
    );
    assert!(
        (300..450).contains(&sampled_high_ratio),
        "ratio=0.75 sampled {} out of {events_per_ratio}",
        sampled_high_ratio
    );
    assert!(
        sampled_high_ratio > sampled_low_ratio,
        "ratio=0.75 sampled {sampled_high_ratio}, ratio=0.25 sampled {sampled_low_ratio}"
    );
}

#[test]
fn dynamic_rate_group_by_partitions_state_by_rate_value() {
    let events_per_rate = 600;
    let mut sampler = Sample::new_with_dynamic(
        "sample".to_string(),
        SampleMode::new_ratio(0.0),
        DynamicSampleFields {
            ratio_field: None,
            rate_field: Some("dynamic_rate".to_string()),
        },
        Some(Template::try_from("{{ service }}").unwrap()),
        None,
        default_sample_rate_key(),
    );

    let mut sampled_rate_2 = 0;
    let mut sampled_rate_3 = 0;
    for _ in 0..events_per_rate {
        for rate in [2_i64, 3_i64] {
            let mut event = Event::Log(LogEvent::from("hello"));
            let log = event.as_mut_log();
            log.insert("service", "service-a");
            log.insert("dynamic_rate", rate);
            if let Some(output) = transform_one(&mut sampler, event) {
                assert_eq!(output.as_log()["sample_rate"], rate.to_string().into());
                if rate == 2 {
                    sampled_rate_2 += 1;
                } else {
                    sampled_rate_3 += 1;
                }
            }
        }
    }

    assert!(
        (220..380).contains(&sampled_rate_2),
        "rate=2 sampled {} out of {events_per_rate}",
        sampled_rate_2
    );
    assert!(
        (120..280).contains(&sampled_rate_3),
        "rate=3 sampled {} out of {events_per_rate}",
        sampled_rate_3
    );
    assert!(
        sampled_rate_2 > sampled_rate_3,
        "rate=2 sampled {sampled_rate_2}, rate=3 sampled {sampled_rate_3}"
    );
}

fn condition_contains(key: &str, needle: &str) -> Condition {
    let vrl_config = VrlConfig {
        source: format!(r#"contains!(."{key}", "{needle}")"#),
        runtime: Default::default(),
    };

    vrl_config
        .build(&Default::default(), &Default::default())
        .expect("should not fail to build VRL condition")
}

fn random_events(n: usize) -> Vec<Event> {
    random_lines(10)
        .take(n)
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect()
}
