use crate::template::Template;
use crate::test_util::components::assert_transform_compliance;
use crate::transforms::sample::config::SampleConfig;
use crate::transforms::test::create_topology;
use crate::transforms::{FunctionTransform, OutputBuffer};
use crate::{
    conditions::{Condition, ConditionalConfig, VrlConfig},
    config::log_schema,
    event::{Event, LogEvent, TraceEvent},
    test_util::random_lines,
    transforms::sample::config::default_sample_rate_key,
    transforms::sample::transform::{Sample, SampleMode},
    transforms::test::transform_one,
};
use approx::assert_relative_eq;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vrl::owned_value_path;

#[tokio::test]
async fn emits_internal_events() {
    assert_transform_compliance(async move {
        let config = SampleConfig {
            rate: None,
            ratio: Some(1.0),
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

fn condition_contains(key: &str, needle: &str) -> Condition {
    let vrl_config = VrlConfig {
        source: format!(r#"contains!(."{}", "{}")"#, key, needle),
        runtime: Default::default(),
    };

    vrl_config
        .build(&Default::default())
        .expect("should not fail to build VRL condition")
}

fn random_events(n: usize) -> Vec<Event> {
    random_lines(10)
        .take(n)
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect()
}
