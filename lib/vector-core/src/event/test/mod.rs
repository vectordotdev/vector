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
            ("Ke$ha".into(), "It's going down, I'm yelling timber".into()),
            (
                "Pitbull".into(),
                "The bigger they are, the harder they fall".into()
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
            ("YRjhxXcg".into(), &Value::from("nw8iM5Jr")),
            ("lZDfzKIL".into(), &Value::from("tOVrjveM")),
            ("o9amkaRY".into(), &Value::from("pGsfG7Nr")),
        ]
    );
}
