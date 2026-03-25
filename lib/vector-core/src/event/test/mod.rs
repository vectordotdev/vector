mod common;
mod serialization;
mod size_of;

use std::collections::HashSet;

use super::*;

#[test]
fn event_iteration() {
    let mut log = LogEvent::default();

    log.insert("\"Ke$ha\"", "It's going down, I'm yelling timber");
    log.insert("Pitbull", "The bigger they are, the harder they fall");

    let all = log
        .all_event_fields()
        .unwrap()
        .map(|(k, v)| (k, v.to_string_lossy()))
        .collect::<HashSet<_>>();
    assert_eq!(
        all,
        vec![
            (
                "Pitbull".parse().expect("compile time values error"),
                "The bigger they are, the harder they fall".into()
            ),
            (
                "\"Ke$ha\"".parse().expect("compile time values error"),
                "It's going down, I'm yelling timber".into()
            ),
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );
}

#[test]
fn event_iteration_order() {
    let mut log = LogEvent::default();
    log.insert("lZDfzKIL", Value::from("tOVrjveM"));
    log.insert("o9amkaRY", Value::from("pGsfG7Nr"));
    log.insert("YRjhxXcg", Value::from("nw8iM5Jr"));

    let collected: Vec<_> = log.all_event_fields().unwrap().collect();
    assert_eq!(
        collected,
        vec![
            ("YRjhxXcg".parse().expect("compile time values error"), &Value::from("nw8iM5Jr")),
            ("lZDfzKIL".parse().expect("compile time values error"), &Value::from("tOVrjveM")),
            ("o9amkaRY".parse().expect("compile time values error"), &Value::from("pGsfG7Nr")),
        ]
    );
}

#[test]
fn special_fields_iterate_and_get_round_trip() {
    let mut log = LogEvent::default();
    log.insert("\"Ke$ha\"", "timber");
    log.insert("normal", "value");
    log.insert("a.nested.path", 42);
    log.insert(
        "arr",
        Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
    );

    // Every path returned by the iterator should resolve back to the same value via get().
    for (path, expected_value) in log.all_event_fields().unwrap() {
        let actual = log.get(&path);
        assert_eq!(
            actual,
            Some(expected_value),
            "round-trip failed for path: {path}"
        );
    }
}
