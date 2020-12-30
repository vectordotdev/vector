use super::*;
use std::{fs, io::Read, path::Path};

fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let mut test_file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(e),
    };

    let mut buf = Vec::new();
    test_file.read_to_end(&mut buf)?;

    Ok(buf)
}

mod insert_get_remove {
    use super::*;

    #[test_env_log::test]
    fn single_field() {
        let mut value = Value::from(BTreeMap::default());
        let key = "root";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_map()[key], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn nested_field() {
        let mut value = Value::from(BTreeMap::default());
        let key = "root.doot";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_map()["root"].as_map()["doot"], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn single_index() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0]";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_array()[0], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn nested_index() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0][0]";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_array()[0].as_array()[0], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn field_index() {
        let mut value = Value::from(BTreeMap::default());
        let key = "root[0]";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_map()["root"].as_array()[0], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn index_field() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0].boot";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_array()[0].as_map()["boot"], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn nested_index_field() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0][0].boot";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_array()[0].as_array()[0].as_map()["boot"], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn field_with_nested_index_field() {
        let mut value = Value::from(BTreeMap::default());
        let key = "root[0][0].boot";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(
            value.as_map()["root"].as_array()[0].as_array()[0].as_map()["boot"],
            marker
        );
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn coalesced_index() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "([0] | [1])";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.as_array()[0], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
    }

    #[test_env_log::test]
    fn coalesced_index_with_tail() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "([0] | [1]).bloop";
        let lookup = LookupBuf::from_str(key).unwrap();
        let mut marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None); // Duplicated on purpose.
        assert_eq!(value.as_array()[0].as_map()["bloop"], marker);
        assert_eq!(value.as_array()[1].as_map()["bloop"], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));

        assert_eq!(value.as_array()[1].as_map()["bloop"], marker);
        assert_eq!(value.get(&lookup).unwrap(), Some(&marker)); // Duplicated on purpose.
        assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker)); // Duplicated on purpose.
        assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker)); // Duplicated on purpose.
    }
}

mod corner_cases {
    use super::*;

    #[test_env_log::test]
    fn remove_prune_map_with_map() {
        let mut value = Value::from(BTreeMap::default());
        let key = "foo.bar";
        let lookup = LookupBuf::from_str(key).unwrap();
        let marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        // Since the `foo` map is now empty, this should get cleaned.
        assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
        assert!(!value.contains("foo"));
    }

    #[test_env_log::test]
    fn remove_prune_map_with_array() {
        let mut value = Value::from(BTreeMap::default());
        let key = "foo[0]";
        let lookup = LookupBuf::from_str(key).unwrap();
        let marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        // Since the `foo` map is now empty, this should get cleaned.
        assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
        assert!(!value.contains("foo"));
    }

    #[test_env_log::test]
    fn remove_prune_array_with_map() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0].bar";
        let lookup = LookupBuf::from_str(key).unwrap();
        let marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        // Since the `foo` map is now empty, this should get cleaned.
        assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
        assert!(!value.contains(0));
    }

    #[test_env_log::test]
    fn remove_prune_array_with_array() {
        let mut value = Value::from(Vec::<Value>::default());
        let key = "[0][0]";
        let lookup = LookupBuf::from_str(key).unwrap();
        let marker = Value::from(true);
        assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
        // Since the `foo` map is now empty, this should get cleaned.
        assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
        assert!(!value.contains(0));
    }
}

// This test iterates over the `tests/data/fixtures/value` folder and:
//   * Ensures the parsed folder name matches the parsed type of the `Value`.
//   * Ensures the `serde_json::Value` to `vector::Value` conversions are harmless. (Think UTF-8 errors)
//
// Basically: This test makes sure we aren't mutilating any content users might be sending.
#[test_env_log::test]
fn json_value_to_value_to_json_value() {
    const FIXTURE_ROOT: &str = "tests/fixtures/value";

    tracing::trace!(?FIXTURE_ROOT, "Opening");
    std::fs::read_dir(FIXTURE_ROOT)
        .unwrap()
        .for_each(|type_dir| match type_dir {
            Ok(type_name) => {
                let path = type_name.path();
                tracing::trace!(?path, "Opening");
                std::fs::read_dir(path)
                    .unwrap()
                    .for_each(|fixture_file| match fixture_file {
                        Ok(fixture_file) => {
                            let path = fixture_file.path();
                            let buf = parse_artifact(&path).unwrap();

                            let serde_value: serde_json::Value =
                                serde_json::from_slice(&*buf).unwrap();
                            let vector_value = Value::from(serde_value.clone());

                            // Validate type
                            let expected_type = type_name
                                .path()
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_string();
                            assert!(
                                match &*expected_type {
                                    "boolean" => vector_value.is_boolean(),
                                    "integer" => vector_value.is_integer(),
                                    "bytes" => vector_value.is_bytes(),
                                    "array" => vector_value.is_array(),
                                    "map" => vector_value.is_map(),
                                    "null" => vector_value.is_null(),
                                    _ => unreachable!("You need to add a new type handler here."),
                                },
                                "Typecheck failure. Wanted {}, got {:?}.",
                                expected_type,
                                vector_value
                            );

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
            _ => panic!("This test should never read Err'ing type folders."),
        })
}
