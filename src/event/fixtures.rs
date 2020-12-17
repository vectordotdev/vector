use shared::{event::*, lookup::*};
use std::convert::{TryFrom, TryInto};
use crate::test_util::open_fixture;
use serde_json::json;

// This test iterates over the `tests/data/fixtures/log_event` folder and:
//   * Ensures the EventLog parsed from bytes and turned into a serde_json::Value are equal to the
//     item being just plain parsed as json.
//
// Basically: This test makes sure we aren't mutilating any content users might be sending.
#[test]
fn json_value_to_vector_log_event_to_json_value() {
    const FIXTURE_ROOT: &str = "tests/data/fixtures/log_event";

    trace!(?FIXTURE_ROOT, "Opening.");
    std::fs::read_dir(FIXTURE_ROOT)
        .unwrap()
        .for_each(|fixture_file| match fixture_file {
            Ok(fixture_file) => {
                let path = fixture_file.path();
                tracing::trace!(?path, "Opening.");
                let serde_value = open_fixture(&path).unwrap();

                let vector_value = LogEvent::try_from(serde_value.clone()).unwrap();
                let serde_value_again: serde_json::Value =
                    vector_value.clone().try_into().unwrap();

                tracing::trace!(
                        ?path,
                        ?serde_value,
                        ?vector_value,
                        ?serde_value_again,
                        "Asserting equal."
                    );
                assert_eq!(serde_value, serde_value_again);
            }
            _ => panic!("This test should never read Err'ing test fixtures."),
        });
}

// We use `serde_json` pointers in this test to ensure we're validating that Vector correctly inputs and outputs things as expected.
#[test]
fn entry() {
    let fixture =
        open_fixture("tests/data/fixtures/log_event/motivatingly-complex.json").unwrap();
    let mut event = LogEvent::try_from(fixture).unwrap();

    let lookup = LookupBuf::from_str("non-existing").unwrap();
    let entry = event.entry(lookup).unwrap();
    let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing lookups."
        );
    entry.or_insert_with(|| fallback.clone().into());
    let json: serde_json::Value = event.clone().try_into().unwrap();
    trace!(?json);
    assert_eq!(json.pointer("/non-existing"), Some(&fallback));

    let lookup = LookupBuf::from_str("nulled").unwrap();
    let entry = event.entry(lookup).unwrap();
    let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, single segment lookups."
        );
    entry.or_insert_with(|| fallback.clone().into());
    let json: serde_json::Value = event.clone().try_into().unwrap();
    assert_eq!(json.pointer("/nulled"), Some(&serde_json::Value::Null));

    let lookup = LookupBuf::from_str("map.basic").unwrap();
    let entry = event.entry(lookup).unwrap();
    let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, double segment lookups."
        );
    entry.or_insert_with(|| fallback.clone().into());
    let json: serde_json::Value = event.clone().try_into().unwrap();
    assert_eq!(
        json.pointer("/map/basic"),
        Some(&serde_json::Value::Bool(true))
    );

    let lookup = LookupBuf::from_str("map.map.buddy").unwrap();
    let entry = event.entry(lookup).unwrap();
    let fallback = json!(
            "If you see this, the `LogEvent::entry` API is not working on existing, multi-segment lookups."
        );
    entry.or_insert_with(|| fallback.clone().into());
    let json: serde_json::Value = event.clone().try_into().unwrap();
    assert_eq!(
        json.pointer("/map/map/buddy"),
        Some(&serde_json::Value::Number((-1).into()))
    );

    let lookup = LookupBuf::from_str("map.map.non-existing").unwrap();
    let entry = event.entry(lookup).unwrap();
    let fallback = json!(
            "If you don't see this, the `LogEvent::entry` API is not working on non-existing multi-segment lookups."
        );
    entry.or_insert_with(|| fallback.clone().into());
    let json: serde_json::Value = event.clone().try_into().unwrap();
    assert_eq!(json.pointer("/map/map/non-existing"), Some(&fallback));
}
