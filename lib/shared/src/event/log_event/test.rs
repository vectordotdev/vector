use super::*;

mod insert_get_remove {
    use super::*;

    #[test_env_log::test]
    fn root() -> crate::Result<()> {
        let mut event = LogEvent::default();
        let lookup = LookupBuf::from_str("root")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner()["root"], value);
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
        assert_eq!(event.inner()["root"].as_map()["doot"], value);
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
        assert_eq!(event.inner()["root"], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("scrubby")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner()["scrubby"], value);
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
        assert_eq!(event.inner()["snoot"].as_map()["loot"], value);
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
        assert_eq!(event.inner()["root"].as_map()["snoot"], value);
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
            event.inner()["root"].as_map()["snoot"].as_map()["leep"],
            value
        );
        assert_eq!(
            event.inner()["root"].as_map()["boot"].as_map()["beep"].as_map()["leep"],
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
        assert_eq!(event.inner()["root"].as_map()["field"], value);
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
            event.inner()["root"].as_map()["field"].as_map()["subfield"],
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
        assert_eq!(event.inner()["root"].as_array()[0], value);
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
        assert_eq!(event.inner()["root"].as_array()[2], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("root[1]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner()["root"].as_array()[1], value);
        assert_eq!(event.get(&lookup), Some(&value));
        assert_eq!(event.get_mut(&lookup), Some(&mut value));
        assert_eq!(event.remove(&lookup, false), Some(value));

        let lookup = LookupBuf::from_str("root[0]")?;
        let mut value = Value::Boolean(true);
        event.insert(lookup.clone(), value.clone());
        assert_eq!(event.inner()["root"].as_array()[0], value);
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
        assert_eq!(event.inner()["root"].as_array()[0].as_array()[0], value);
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
            event.inner()["root"].as_array()[0].as_map()["nested"],
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
            event.inner()["root"].as_array()[10].as_map()["nested"].as_array()[10].as_map()["more"]
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
    let expected = Lookup::from_str("snooper").unwrap();
    assert_eq!(keys[0], expected);
    assert_eq!(pairs[0].0, expected);
    let expected = Lookup::from_str("snooper.booper").unwrap();
    assert_eq!(keys[1], expected);
    assert_eq!(pairs[1].0, expected);
    // Ensure a new array element that was injected is iterated over.
    let expected = Lookup::from_str("snooper.booper[0]").unwrap();
    assert_eq!(keys[2], expected);
    assert_eq!(pairs[2].0, expected);
    let expected = Lookup::from_str("snooper.booper[1]").unwrap();
    assert_eq!(keys[3], expected);
    assert_eq!(pairs[3].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][0]").unwrap();
    assert_eq!(keys[4], expected);
    assert_eq!(pairs[4].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][1]").unwrap();
    assert_eq!(keys[5], expected);
    assert_eq!(pairs[5].0, expected);
    let expected = Lookup::from_str("snooper.booper[1][2]").unwrap();
    assert_eq!(keys[6], expected);
    assert_eq!(pairs[6].0, expected);
    // Try inside arrays now.
    let expected = Lookup::from_str("whomp").unwrap();
    assert_eq!(keys[7], expected);
    assert_eq!(pairs[7].0, expected);
    let expected = Lookup::from_str("whomp[0]").unwrap();
    assert_eq!(keys[8], expected);
    assert_eq!(pairs[8].0, expected);
    let expected = Lookup::from_str("whomp[1]").unwrap();
    assert_eq!(keys[9], expected);
    assert_eq!(pairs[9].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp").unwrap();
    assert_eq!(keys[10], expected);
    assert_eq!(pairs[10].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp[0]").unwrap();
    assert_eq!(keys[11], expected);
    assert_eq!(pairs[11].0, expected);
    let expected = Lookup::from_str("whomp[1].glomp[1]").unwrap();
    assert_eq!(keys[12], expected);
    assert_eq!(pairs[12].0, expected);
    let expected = Lookup::from_str("zoop").unwrap();
    assert_eq!(keys[13], expected);
    assert_eq!(pairs[13].0, expected);

    Ok(())
}
