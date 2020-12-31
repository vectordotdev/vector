use crate::{event::*, log_event, lookup::*, map, test::open_fixture};
use serde_json::json;
use std::collections::BTreeMap;
use tracing::trace;

mod insert_get_remove {
    use super::*;

    #[test_env_log::test]
    fn itself() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str(".")?;
        let mut value = Value::Map(BTreeMap::default());
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner(), &value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), None); // Cannot remove self from Event.
        Ok(())
    }

    #[test_env_log::test]
    fn root() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn quoted_from_str() -> crate::Result<()> {
        // In this test, we make sure the quotes are stripped, since it's a parsed lookup.
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root.\"doot\"")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_map()["doot"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn root_with_buddy() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("scrubby")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["scrubby"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn coalesced_root() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("(snoot | boot).loot")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["snoot"].as_map()["loot"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("boot")?;
        assert_eq!(event.get(&lookup), None);

        Ok(())
    }

    #[test_env_log::test]
    fn coalesced_nested() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root.(snoot | boot)")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_map()["snoot"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("root.boot")?;
        assert_eq!(event.get(&lookup), None);

        Ok(())
    }

    #[test_env_log::test]
    fn coalesced_with_nesting() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root.(snoot | boot.beep).leep")?;
        let mut value = Value::Boolean(true);

        // This is deliberately duplicated!!! Because it's a coalesce both fields will be filled.
        // This is the point of the test!
        event.insert(lookup.clone(), value.clone());
        event.insert(lookup.clone(), value.clone());

        assert_eq!(
            event.inner().as_map()["root"].as_map()["snoot"].as_map()["leep"],
            value
        );
        assert_eq!(
            event.inner().as_map()["root"].as_map()["boot"].as_map()["beep"].as_map()["leep"],
            value
        );

        // This repeats, because it's the purpose of the test!
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value.clone()));
        // Now that we removed one, we will get the other.
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        Ok(())
    }
    #[test_env_log::test]
    fn map_field() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root.field")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_map()["field"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn nested_map_field() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root.field.subfield")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(
            event.inner().as_map()["root"].as_map()["field"].as_map()["subfield"],
            value
        );
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn array_field() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root[0]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_array()[0], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn array_reverse_population() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root[2]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_array()[2], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("root[1]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_array()[1], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("root[0]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner().as_map()["root"].as_array()[0], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn array_field_nested_array() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root[0][0]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(
            event.inner().as_map()["root"].as_array()[0].as_array()[0],
            value
        );
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn array_field_nested_map() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root[0].nested")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(
            event.inner().as_map()["root"].as_array()[0].as_map()["nested"],
            value
        );
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }

    #[test_env_log::test]
    fn perverse() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str(
            "root[10].nested[10].more[9].than[8].there[7][6][5].we.go.friends.look.at.this",
        )?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(
            event.inner().as_map()["root"].as_array()[10].as_map()["nested"].as_array()[10]
                .as_map()["more"]
                .as_array()[9]
                .as_map()["than"]
                .as_array()[8]
                .as_map()["there"]
                .as_array()[7]
                .as_array()[6]
                .as_array()[5]
                .as_map()["we"]
                .as_map()["go"]
                .as_map()["friends"]
                .as_map()["look"]
                .as_map()["at"]
                .as_map()["this"],
            value
        );
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));
        Ok(())
    }
}

mod corner_cases {
    use super::*;

    // Prune on deeply nested values is tested in `value.rs`, but we must test root values here.
    #[test_env_log::test]
    fn pruning() -> crate::Result<()> {
        let mut event = crate::log_event! {
            LookupBuf::from("foo.bar.baz") => 1,
        }
        .into_log();
        assert_eq!(
            event.remove(Lookup::from("foo.bar.baz"), true),
            Some(Value::from(1))
        );
        assert!(!event.contains(Lookup::from("foo.bar")));
        assert!(!event.contains(Lookup::from("foo")));

        let mut event = crate::log_event! {
            LookupBuf::from("foo.bar") => 1,
        }
        .into_log();
        assert_eq!(
            event.remove(Lookup::from("foo.bar"), true),
            Some(Value::from(1))
        );
        assert!(!event.contains(Lookup::from("foo")));

        Ok(())
    }

    // While authors should prefer to set an array via `event.insert(lookup_to_array, array)`,
    // there are some cases where we want to insert 1 by one. Make sure this can happen.
    #[test_env_log::test]
    fn iteratively_populate_array() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookups = vec![
            LookupBuf::from_str("root.nested[0]")?,
            LookupBuf::from_str("root.nested[1]")?,
            LookupBuf::from_str("root.nested[2]")?,
            LookupBuf::from_str("other[1][0]")?,
            LookupBuf::from_str("other[1][1].a")?,
            LookupBuf::from_str("other[1][1].b")?,
        ];
        let value = Value::Boolean(true);
        for lookup in lookups.clone() {
            event.insert(lookup, value.clone());
        }
        let pairs = event.keys(true).collect::<Vec<_>>();
        for lookup in lookups {
            assert!(
                pairs.contains(&lookup.clone_lookup()),
                "Failed while looking for {}",
                lookup
            );
        }
        Ok(())
    }

    // While authors should prefer to set an array via `event.insert(lookup_to_array, array)`,
    // there are some cases where we want to insert 1 by one. Make sure this can happen.
    #[test_env_log::test]
    fn iteratively_populate_array_reverse() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookups = vec![
            LookupBuf::from_str("root.nested[1]")?,
            LookupBuf::from_str("root.nested[0]")?,
            LookupBuf::from_str("other[1][1]")?,
            LookupBuf::from_str("other[0][1].a")?,
        ];
        let value = Value::Boolean(true);
        for lookup in lookups.clone() {
            event.insert(lookup, value.clone());
        }
        let pairs = event.keys(false).collect::<Vec<_>>();
        for lookup in lookups.clone() {
            assert!(
                pairs.contains(&lookup.clone_lookup()),
                "Failed while looking for {} in {}",
                lookup,
                pairs
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }
        Ok(())
    }

    // While authors should prefer to set an map via `event.insert(lookup_to_map, map)`,
    // there are some cases where we want to insert 1 by one. Make sure this can happen.
    #[test_env_log::test]
    fn iteratively_populate_map() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookups = vec![
            LookupBuf::from_str("root.one")?,
            LookupBuf::from_str("root.two")?,
            LookupBuf::from_str("root.three.a")?,
            LookupBuf::from_str("root.three.b")?,
            LookupBuf::from_str("root.three.c")?,
            LookupBuf::from_str("root.four[0]")?,
            LookupBuf::from_str("root.four[1]")?,
            LookupBuf::from_str("root.four[2]")?,
        ];
        let value = Value::Boolean(true);
        for lookup in lookups.clone() {
            event.insert(lookup, value.clone());
        }
        // Note: Two Lookups are only the same if the string slices underneath are too.
        //       LookupBufs this rule does not apply.
        let pairs = event.keys(true).map(|k| k.into_buf()).collect::<Vec<_>>();
        for lookup in lookups {
            assert!(
                pairs.contains(&lookup),
                "Failed while looking for {}",
                lookup
            );
        }
        Ok(())
    }

    // Here we ensure that inserts always get inserted, even if there is an existing parent
    // which is not able to accept the value.
    //
    // This primarily exists because `LogEvent::insert()` returns an option which can't communicate
    // failure, so inserts must always work, or fail in unsurprising ways.
    #[test_env_log::test]
    fn insert_clobbers_existing_parents() -> crate::Result<()> {
        let mut event = log_event! {
            "root" => true,
        }
        .into_log();
        // These lookups iteratively overwrite the previously inserted value.
        // All these should succeed and not return a value (They're overwriting stuff).
        let lookups = vec![
            LookupBuf::from_str("root.one")?,
            LookupBuf::from_str("root.one.two")?,
            LookupBuf::from_str("root.one.two[3]")?,
            LookupBuf::from_str("root.one.two[3].four")?,
        ];
        let value = Value::Boolean(true);
        for lookup in lookups.clone() {
            event.insert(lookup.clone(), value.clone());
            let fetched = event.get(&lookup);
            assert_eq!(
                fetched,
                Some(&value),
                "Insert into {} did not yield value of parent ({:?})",
                lookup,
                value,
            );
        }
        Ok(())
    }
}

#[test_env_log::test]
fn keys_and_pairs() -> crate::Result<()> {
    let mut event = LogEvent::default();
    // We opt for very small arrays here to avoid having to iterate a bunch.
    let lookup = LookupBuf::from_str("snooper.booper[1][2]")?;
    event.insert(lookup, Value::Null);
    let lookup = LookupBuf::from_str("whomp[1].glomp[1]")?;
    event.insert(lookup, Value::Null);
    let lookup = LookupBuf::from_str("zoop")?;
    event.insert(lookup, Value::Null);

    // Collect and sort since we don't want a flaky test on iteration do we?
    let mut keys = event.keys(false).collect::<Vec<_>>();
    keys.sort();
    let mut pairs = event.pairs(false).collect::<Vec<_>>();
    pairs.sort_by(|v, x| v.0.cmp(&x.0));

    // Ensure a new field element that was injected is iterated over.
    let mut i = 0;
    let expected = Lookup::from_str(".").unwrap();
    assert_eq!(keys[i], expected);
    let expected = Lookup::from_str("snooper").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("snooper.booper").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    // Ensure a new array element that was injected is iterated over.
    let expected = Lookup::from_str("snooper.booper[0]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("snooper.booper[1]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][0]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][1]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][2]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    // Try inside arrays now.
    let expected = Lookup::from_str("whomp").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("whomp[0]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("whomp[1]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp[0]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp[1]").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);
    let expected = Lookup::from_str("zoop").unwrap();
    i += 1;
    assert_eq!(keys[i], expected);
    assert_eq!(pairs[i].0, expected);

    Ok(())
}

// This test iterates over the `tests/data/fixtures/log_event` folder and:
//   * Ensures the EventLog parsed from bytes and turned into a serde_json::Value are equal to the
//     item being just plain parsed as json.
//
// Basically: This test makes sure we aren't mutilating any content users might be sending.
#[test_env_log::test]
fn json_value_to_vector_log_event_to_json_value() {
    const FIXTURE_ROOT: &str = "tests/fixtures/log_event";

    trace!(?FIXTURE_ROOT, "Opening.");
    std::fs::read_dir(FIXTURE_ROOT)
        .unwrap()
        .for_each(|fixture_file| match fixture_file {
            Ok(fixture_file) => {
                let path = fixture_file.path();
                tracing::trace!(?path, "Opening.");
                let serde_value = open_fixture(&path).unwrap();

                let vector_value = LogEvent::try_from(serde_value.clone()).unwrap();
                let serde_value_again: serde_json::Value = vector_value.clone().try_into().unwrap();

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
#[test_env_log::test]
fn entry() {
    let fixture = open_fixture("tests/fixtures/log_event/motivatingly-complex.json").unwrap();
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

mod remap {
    use super::*;
    use remap_lang::Object;
    use std::collections::BTreeMap;

    #[test_env_log::test]
    fn object_get() {
        let cases = vec![
            (map![], vec![], Ok(Some(map![].into()))),
            (
                map!["foo": "bar"],
                vec![],
                Ok(Some(map!["foo": "bar"].into())),
            ),
            (
                map!["foo": "bar"],
                vec![remap_lang::Segment::Field(remap_lang::Field::Regular(
                    "foo".to_owned(),
                ))],
                Ok(Some("bar".into())),
            ),
            (
                map!["foo": "bar"],
                vec![remap_lang::Segment::Field(remap_lang::Field::Regular(
                    "bar".to_owned(),
                ))],
                Ok(None),
            ),
            (
                map!["foo": vec![map!["bar": true]]],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                    remap_lang::Segment::Field(remap_lang::Field::Regular("bar".to_owned())),
                ],
                Ok(Some(true.into())),
            ),
            (
                map!["foo": map!["bar baz": map!["baz": 2]]],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Coalesce(vec![
                        remap_lang::Field::Regular("qux".to_owned()),
                        remap_lang::Field::Quoted("bar baz".to_owned()),
                    ]),
                    remap_lang::Segment::Field(remap_lang::Field::Regular("baz".to_owned())),
                ],
                Ok(Some(2.into())),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: BTreeMap<String, Value> = value;
            let event = LogEvent::from(value);
            let path = remap_lang::Path::new_unchecked(segments);

            assert_eq!(
                Object::get(&event, &path),
                expect,
                "Expected {:?} to return {:?} in {:?}",
                path,
                expect,
                event
            )
        }
    }

    #[test_env_log::test]
    fn object_insert() {
        let cases = vec![
            (
                map!["foo": "bar"],
                vec![],
                map!["baz": "qux"].into(),
                map!["baz": "qux"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![remap_lang::Segment::Field(remap_lang::Field::Regular(
                    "foo".to_owned(),
                ))],
                "baz".into(),
                map!["foo": "baz"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(2),
                    remap_lang::Segment::Field(remap_lang::Field::Quoted("bar baz".to_owned())),
                    remap_lang::Segment::Field(remap_lang::Field::Regular("a".to_owned())),
                    remap_lang::Segment::Field(remap_lang::Field::Regular("b".to_owned())),
                ],
                true.into(),
                map![
                    "foo":
                        vec![
                            Value::Null,
                            Value::Null,
                            map!["bar baz": map!["a": map!["b": true]],].into()
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": vec![0, 1, 2]],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(5),
                ],
                "baz".into(),
                map![
                    "foo":
                        vec![
                            0.into(),
                            1.into(),
                            2.into(),
                            Value::Null,
                            Value::Null,
                            Value::from("baz"),
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![])],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into()])],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                "baz".into(),
                map!["foo": Value::Array(vec!["baz".into(), 1.into()])],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(1),
                ],
                "baz".into(),
                map!["foo": Value::Array(vec![0.into(), "baz".into()])],
                Ok(()),
            ),
        ];

        for (object, segments, value, expect, result) in cases {
            let object: BTreeMap<String, Value> = object;
            let mut event = LogEvent::from(object);
            let expect = LogEvent::from(expect);
            let value: remap::Value = value;
            let path = remap_lang::Path::new_unchecked(segments);

            assert_eq!(
                remap_lang::Object::insert(&mut event, &path, value.clone().into()),
                result,
                "Result of {:?}::insert({:?},{:?}) was not {:?}.",
                event,
                path,
                value,
                result
            );
            assert_eq!(event, expect);
            assert_eq!(remap::Object::get(&event, &path), Ok(Some(value.into())));
        }
    }

    #[test_env_log::test]
    fn object_remove() {
        let cases = vec![
            (
                map!["foo": "bar"],
                vec![remap_lang::Segment::Field(remap_lang::Field::Regular(
                    "foo".to_owned(),
                ))],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar"],
                vec![remap_lang::Segment::Coalesce(vec![
                    remap_lang::Field::Quoted("foo bar".to_owned()),
                    remap_lang::Field::Regular("foo".to_owned()),
                ])],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"],
                vec![],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"],
                vec![],
                true,
                Some(map![].into()),
            ),
            (
                map!["foo": vec![0]],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                false,
                Some(map!["foo": Value::Array(vec![])].into()),
            ),
            (
                map!["foo": vec![0]],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                true,
                Some(map![].into()),
            ),
            (
                map! {"foo": map!{"bar baz": vec![0]}, "bar": "baz"},
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Field(remap_lang::Field::Quoted("bar baz".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                false,
                Some(map!["foo": map!["bar baz": Value::Array(vec![])], "bar": "baz"].into()),
            ),
            (
                map!["foo": map!["bar baz": vec![0]], "bar": "baz"],
                vec![
                    remap_lang::Segment::Field(remap_lang::Field::Regular("foo".to_owned())),
                    remap_lang::Segment::Field(remap_lang::Field::Quoted("bar baz".to_owned())),
                    remap_lang::Segment::Index(0),
                ],
                true,
                Some(map!["bar": "baz"].into()),
            ),
        ];

        for (object, segments, compact, expect) in cases {
            let mut event = LogEvent::from(object);
            let path = remap_lang::Path::new_unchecked(segments);

            assert_eq!(
                remap_lang::Object::remove(&mut event, &path, compact),
                Ok(())
            );
            assert_eq!(
                remap_lang::Object::get(&event, &remap_lang::Path::root()),
                Ok(expect)
            )
        }
    }

    #[test_env_log::test]
    fn object_paths() {
        use remap_lang::Object;
        use std::str::FromStr;

        let cases = vec![
            (map! {}, Ok(vec!["."])),
            (
                map! { "\"foo bar baz\"": "bar" },
                Ok(vec![r#"."foo bar baz""#]),
            ),
            (
                map! { "foo": "bar", "baz": "qux" },
                Ok(vec![".baz", ".foo"]),
            ),
            (map! { "foo": map!{ "bar": "baz" }}, Ok(vec![".foo.bar"])),
            (map! { "a": vec![0, 1] }, Ok(vec![".a[0]", ".a[1]"])),
            (
                map! {
                    "a": map!{ "b": "c" },
                    "d": 12,
                    "e": vec![
                        map!{"f": 1},
                        map!{"g": 2},
                        map!{"h": 3},
                    ],
                },
                Ok(vec![".a.b", ".d", ".e[0].f", ".e[1].g", ".e[2].h"]),
            ),
            (
                map! {
                    "a": vec![
                        map!{
                            "b": vec![map!{"c": map!{"d": map!{"e": vec![vec![0, 1]]}}}],
                        },
                    ],
                },
                Ok(vec![".a[0].b[0].c.d.e[0][0]", ".a[0].b[0].c.d.e[0][1]"]),
            ),
        ];

        for (object, expect) in cases {
            let object: BTreeMap<String, Value> = object;
            let event = LogEvent::from(object);

            assert_eq!(
                event.paths(),
                expect.map(|vec| vec
                    .iter()
                    .map(|s| remap_lang::Path::from_str(s).unwrap())
                    .collect())
            );
        }
    }
}
