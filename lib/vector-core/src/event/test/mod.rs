mod common;
mod serialization;
mod size_of;

use std::collections::HashSet;

use super::*;

#[test]
fn event_iteration() {
    let mut event = Event::new_empty_log();

    event
        .as_mut_log()
        .insert("Ke$ha", "It's going down, I'm yelling timber");
    event
        .as_mut_log()
        .insert("Pitbull", "The bigger they are, the harder they fall");

    let all = event
        .as_log()
        .all_fields()
        .map(|(k, v)| (k, v.to_string_lossy()))
        .collect::<HashSet<_>>();
    assert_eq!(
        all,
        vec![
            (
                String::from("Ke$ha"),
                "It's going down, I'm yelling timber".to_string()
            ),
            (
                String::from("Pitbull"),
                "The bigger they are, the harder they fall".to_string()
            ),
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );
}

#[test]
fn event_iteration_order() {
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();
    log.insert("lZDfzKIL", Value::from("tOVrjveM"));
    log.insert("o9amkaRY", Value::from("pGsfG7Nr"));
    log.insert("YRjhxXcg", Value::from("nw8iM5Jr"));

    let collected: Vec<_> = log.all_fields().collect();
    assert_eq!(
        collected,
        vec![
            (String::from("YRjhxXcg"), &Value::from("nw8iM5Jr")),
            (String::from("lZDfzKIL"), &Value::from("tOVrjveM")),
            (String::from("o9amkaRY"), &Value::from("pGsfG7Nr")),
        ]
    );
}
