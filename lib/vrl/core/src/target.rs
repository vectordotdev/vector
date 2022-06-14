use lookup::LookupBuf;
use value::{Secrets, Value};

/// Any target object you want to remap using VRL has to implement this trait.
pub trait Target: std::fmt::Debug + MetadataTarget + SecretTarget {
    /// Insert a given [`Value`] in the provided [`Target`].
    ///
    /// The `path` parameter determines _where_ in the given target the value
    /// should be inserted.
    ///
    /// A path consists of "path segments". Each segment can be one of:
    ///
    /// * regular path segments:
    ///
    ///   ```txt
    ///   .foo.bar.baz
    ///   ```
    ///
    /// * quoted path segments:
    ///
    ///   ```txt
    ///   .foo."bar.baz"
    ///   ```
    ///
    /// * coalesced path segments:
    ///
    ///   ```txt
    ///   .foo.(bar | "bar.baz").qux
    ///   ```
    ///
    /// * path indices:
    ///
    ///   ```txt
    ///   .foo[2][-1]
    ///   ```
    ///
    /// When inserting into a coalesced path, the implementor is encouraged to
    /// insert into the right-most segment if none exists, but can return an
    /// error if needed.
    fn target_insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String>;

    /// Get a value for a given path, or `None` if no value is found.
    ///
    /// See [`Target::insert`] for more details.
    fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String>;

    /// Get a mutable reference to the value for a given path, or `None` if no
    /// value is found.
    ///
    /// See [`Target::insert`] for more details.
    fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String>;

    /// Remove the given path from the object.
    ///
    /// Returns the removed object, if any.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well, cascading up to the root.
    fn target_remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String>;
}

pub trait MetadataTarget {
    fn get_metadata(&self, _path: &LookupBuf) -> Result<Option<Value>, String>;

    fn set_metadata(&mut self, _path: &LookupBuf, _value: Value) -> Result<(), String>;

    fn remove_metadata(&mut self, _path: &LookupBuf) -> Result<(), String>;
}

pub trait SecretTarget {
    fn get_secret(&self, key: &str) -> Option<&str>;

    fn insert_secret(&mut self, key: &str, value: &str);

    fn remove_secret(&mut self, key: &str);
}

#[derive(Debug)]
pub struct TargetValueRef<'a> {
    pub value: &'a mut Value,
    pub metadata: &'a mut Value,
    pub secrets: &'a mut Secrets,
}

impl Target for TargetValueRef<'_> {
    fn target_insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.value.insert_by_path(path, value);
        Ok(())
    }

    fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String> {
        Ok(self.value.get_by_path(path))
    }

    fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String> {
        Ok(self.value.get_by_path_mut(path))
    }

    fn target_remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String> {
        Ok(self.value.remove_by_path(path, compact))
    }
}

impl MetadataTarget for TargetValueRef<'_> {
    fn get_metadata(&self, path: &LookupBuf) -> Result<Option<Value>, String> {
        Ok(self.metadata.get_by_path(path).cloned())
    }

    fn set_metadata(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.metadata.insert_by_path(path, value);
        Ok(())
    }

    fn remove_metadata(&mut self, path: &LookupBuf) -> Result<(), String> {
        self.metadata.remove_by_path(path, true);
        Ok(())
    }
}

impl SecretTarget for TargetValueRef<'_> {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.secrets.get_secret(key)
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.secrets.insert_secret(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.secrets.remove_secret(key);
    }
}

#[derive(Debug)]
pub struct TargetValue {
    pub value: Value,
    pub metadata: Value,
    pub secrets: Secrets,
}

impl Target for TargetValue {
    fn target_insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.value.insert_by_path(path, value);
        Ok(())
    }

    fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String> {
        Ok(self.value.get_by_path(path))
    }

    fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String> {
        Ok(self.value.get_by_path_mut(path))
    }

    fn target_remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String> {
        Ok(self.value.remove_by_path(path, compact))
    }
}

impl MetadataTarget for TargetValue {
    fn get_metadata(&self, path: &LookupBuf) -> Result<Option<Value>, String> {
        Ok(self.metadata.get_by_path(path).cloned())
    }

    fn set_metadata(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.metadata.insert_by_path(path, value);
        Ok(())
    }

    fn remove_metadata(&mut self, path: &LookupBuf) -> Result<(), String> {
        self.metadata.remove_by_path(path, true);
        Ok(())
    }
}

impl SecretTarget for TargetValue {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.secrets.get_secret(key)
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.secrets.insert_secret(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.secrets.remove_secret(key);
    }
}

impl SecretTarget for Secrets {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.get(key).map(|value| value.as_ref())
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.insert(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.remove(key);
    }
}

#[cfg(any(test, feature = "test"))]
mod value_target_impl {
    use super::*;

    impl Target for Value {
        fn target_insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
            self.insert_by_path(path, value);
            Ok(())
        }

        fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String> {
            Ok(self.get_by_path(path))
        }

        fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String> {
            Ok(self.get_by_path_mut(path))
        }

        fn target_remove(
            &mut self,
            path: &LookupBuf,
            compact: bool,
        ) -> Result<Option<Value>, String> {
            Ok(self.remove_by_path(path, compact))
        }
    }

    impl MetadataTarget for Value {
        fn get_metadata(&self, _path: &LookupBuf) -> Result<Option<Value>, String> {
            panic!("Value has no metadata. Use `TargetValue` instead.")
        }

        fn set_metadata(&mut self, _path: &LookupBuf, _value: Value) -> Result<(), String> {
            panic!("Value has no metadata. Use `TargetValue` instead.")
        }

        fn remove_metadata(&mut self, _path: &LookupBuf) -> Result<(), String> {
            panic!("Value has no metadata. Use `TargetValue` instead.")
        }
    }

    impl SecretTarget for Value {
        fn get_secret(&self, _key: &str) -> Option<&str> {
            panic!("Value has no secrets. Use `TargetValue` instead.")
        }

        fn insert_secret(&mut self, _key: &str, _value: &str) {
            panic!("Value has no secrets. Use `TargetValue` instead.")
        }

        fn remove_secret(&mut self, _key: &str) {
            panic!("Value has no secrets. Use `TargetValue` instead.")
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] // tests

    use lookup::{FieldBuf, SegmentBuf};

    use super::*;
    use crate::value;

    #[test]
    fn target_get() {
        let cases = vec![
            (value!(true), vec![], Ok(Some(value!(true)))),
            (value!(true), vec![SegmentBuf::from("foo")], Ok(None)),
            (value!({}), vec![], Ok(Some(value!({})))),
            (value!({foo: "bar"}), vec![], Ok(Some(value!({foo: "bar"})))),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                Ok(Some(value!("bar"))),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("bar")],
                Ok(None),
            ),
            (
                value!([1, 2, 3, 4, 5]),
                vec![SegmentBuf::from(1)],
                Ok(Some(value!(2))),
            ),
            (
                value!({foo: [{bar: true}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("bar"),
                ],
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2}}}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(vec![FieldBuf::from("qux"), FieldBuf::from(r#""bar baz""#)]),
                    SegmentBuf::from("baz"),
                ],
                Ok(Some(value!(2))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: Value = value;
            let target = TargetValue {
                value,
                metadata: value!({}),
                secrets: Secrets::new(),
            };
            let path = LookupBuf::from_segments(segments);

            assert_eq!(target.target_get(&path).map(|v| v.cloned()), expect);
        }
    }

    #[test]
    fn target_insert() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![],
                value!({baz: "qux"}),
                value!({baz: "qux"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                true.into(),
                value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                value!({foo: [{bar: "baz"}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("baz"),
                ],
                true.into(),
                value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                value!({foo: {bar: "baz"}}),
                vec![SegmentBuf::from("bar"), SegmentBuf::from("baz")],
                true.into(),
                value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                "baz".into(),
                value!({foo: "baz"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(2),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from("a"),
                    SegmentBuf::from("b"),
                ],
                true.into(),
                value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
                Ok(()),
            ),
            /*
            (
                value!({foo: [0, 1, 2]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(5)],
                "baz".into(),
                value!({foo: [0, 1, 2, null, null, "baz"]}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: []}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz", 1]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(1)],
                "baz".into(),
                value!({foo: [0, "baz"]}),
                Ok(()),
            ),
            */
        ];

        for (target, segments, value, expect, result) in cases {
            let mut target = TargetValue {
                value: target,
                metadata: value!({}),
                secrets: Secrets::new(),
            };
            println!("Inserting at {:?}", segments);
            let path = LookupBuf::from_segments(segments);

            assert_eq!(
                Target::target_insert(&mut target, &path, value.clone()),
                result
            );
            assert_eq!(target.value, expect);
            assert_eq!(
                Target::target_get(&target, &path).map(|v| v.cloned()),
                Ok(Some(value))
            );
        }
    }

    #[test]
    fn target_remove() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                false,
                None,
                Some(value!({foo: "bar"})),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::coalesce(vec![
                    FieldBuf::from(r#""foo bar""#),
                    FieldBuf::from("foo"),
                ])],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                false,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                true,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                false,
                Some(value!(0)),
                Some(value!({foo: []})),
            ),
            (
                value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                true,
                Some(value!(0)),
                Some(value!({})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                false,
                Some(value!(0)),
                Some(value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                true,
                Some(value!(0)),
                Some(value!({bar: "baz"})),
            ),
        ];

        for (target, segments, compact, value, expect) in cases {
            let path = LookupBuf::from_segments(segments);

            let mut target = TargetValue {
                value: target,
                metadata: value!({}),
                secrets: Secrets::new(),
            };
            assert_eq!(
                Target::target_remove(&mut target, &path, compact),
                Ok(value)
            );
            assert_eq!(
                Target::target_get(&target, &LookupBuf::root()).map(|v| v.cloned()),
                Ok(expect)
            );
        }
    }
}
