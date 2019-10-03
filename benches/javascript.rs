use criterion::{criterion_group, Benchmark, Criterion};
use indexmap::IndexMap;
use transforms::javascript::JavaScript;
use vector::{
    topology::config::TransformConfig,
    transforms::{self, Transform},
    Event,
};

fn add_fields(c: &mut Criterion) {
    let num_events: usize = 100_000;

    let key = "the key";
    let value = "this is the value";

    let key_atom = key.into();
    let value_bytes = value.into();
    let key_atom2 = key.into();
    let value_bytes2 = value.into();
    let key_atom3 = key.into();
    let value_bytes3 = value.into();

    c.bench(
        "javascript_add_fields",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let mut map = IndexMap::new();
                    map.insert(key.into(), toml::value::Value::String(value.to_owned()));
                    transforms::add_fields::AddFields::new(map)
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom], value_bytes);
                    }
                },
            )
        })
        .with_function("javascript_with_copying", move |b| {
            b.iter_with_setup(
                || {
                    let config = format!(
                        r#"
                        source = "event => ({{...event, ['{}']: '{}'}})"
                        "#,
                        key, value
                    );
                    JavaScript::new(toml::from_str(&config).unwrap()).unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom2], value_bytes2);
                    }
                },
            )
        })
        .with_function("javascript_without_copying", move |b| {
            b.iter_with_setup(
                || {
                    let config = format!(
                        r#"
                        source = "event => {{ event['{}'] = '{}'; return event }}"
                        "#,
                        key, value
                    );
                    JavaScript::new(toml::from_str(&config).unwrap()).unwrap()
                },
                |mut transform| {
                    for _ in 0..num_events {
                        let event = Event::new_empty_log();
                        let event = transform.transform(event).unwrap();
                        assert_eq!(event.as_log()[&key_atom3], value_bytes3);
                    }
                },
            )
        })
        .sample_size(10),
    );
}

fn field_filter(c: &mut Criterion) {
    let num_events: usize = 100_000;

    c.bench(
        "javascript_field_filter",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let transform = transforms::field_filter::FieldFilterConfig {
                        field: "the_field".to_string(),
                        value: "0".to_string(),
                    }
                    .build()
                    .unwrap();

                    let events: Vec<Event> = (0..num_events)
                        .map(|i| {
                            let mut event = Event::new_empty_log();
                            event
                                .as_mut_log()
                                .insert_explicit("the_field".into(), (i % 10).to_string().into());
                            event
                        })
                        .collect();
                    (transform, events)
                },
                |(mut transform, events)| {
                    let num = events
                        .into_iter()
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .with_function("javascript", move |b| {
            b.iter_with_setup(
                || {
                    let config = r#"
                    source = "event => (event.the_field !== '0') ? null : event"
                    "#;
                    let transform = JavaScript::new(toml::from_str(config).unwrap()).unwrap();

                    let events: Vec<Event> = (0..num_events)
                        .map(|i| {
                            let mut event = Event::new_empty_log();
                            event
                                .as_mut_log()
                                .insert_explicit("the_field".into(), (i % 10).to_string().into());
                            event
                        })
                        .collect();
                    (transform, events)
                },
                |(mut transform, events)| {
                    let num = events
                        .into_iter()
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_events / 10);
                },
            )
        })
        .sample_size(10),
    );
}

criterion_group!(javascript, add_fields, field_filter);
