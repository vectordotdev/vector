use std::convert::AsRef;

use lookup::lookup_v2::OwnedTargetPath;
use lookup::PathPrefix;
use value::{Secrets, Value};

/// Any target object you want to remap using VRL has to implement this trait.
pub trait Target: std::fmt::Debug + SecretTarget {
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
    fn target_insert(&mut self, path: &OwnedTargetPath, value: Value) -> Result<(), String>;

    /// Get a value for a given path, or `None` if no value is found.
    ///
    /// See [`Target::target_insert`] for more details.
    fn target_get(&self, path: &OwnedTargetPath) -> Result<Option<&Value>, String>;

    /// Get a mutable reference to the value for a given path, or `None` if no
    /// value is found.
    ///
    /// See [`Target::target_insert`] for more details.
    fn target_get_mut(&mut self, path: &OwnedTargetPath) -> Result<Option<&mut Value>, String>;

    /// Remove the given path from the object.
    ///
    /// Returns the removed object, if any.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well, cascading up to the root.
    fn target_remove(
        &mut self,
        path: &OwnedTargetPath,
        compact: bool,
    ) -> Result<Option<Value>, String>;
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
    fn target_insert(&mut self, target_path: &OwnedTargetPath, value: Value) -> Result<(), String> {
        match target_path.prefix {
            PathPrefix::Event => self.value.insert(&target_path.path, value),
            PathPrefix::Metadata => self.metadata.insert(&target_path.path, value),
        };
        Ok(())
    }

    fn target_get(&self, target_path: &OwnedTargetPath) -> Result<Option<&Value>, String> {
        let value = match target_path.prefix {
            PathPrefix::Event => self.value.get(&target_path.path),
            PathPrefix::Metadata => self.metadata.get(&target_path.path),
        };
        Ok(value)
    }

    fn target_get_mut(
        &mut self,
        target_path: &OwnedTargetPath,
    ) -> Result<Option<&mut Value>, String> {
        let value = match target_path.prefix {
            PathPrefix::Event => self.value.get_mut(&target_path.path),
            PathPrefix::Metadata => self.metadata.get_mut(&target_path.path),
        };
        Ok(value)
    }

    fn target_remove(
        &mut self,
        target_path: &OwnedTargetPath,
        compact: bool,
    ) -> Result<Option<Value>, String> {
        let prev_value = match target_path.prefix {
            PathPrefix::Event => self.value.remove(&target_path.path, compact),
            PathPrefix::Metadata => self.metadata.remove(&target_path.path, compact),
        };
        Ok(prev_value)
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
    fn target_insert(&mut self, target_path: &OwnedTargetPath, value: Value) -> Result<(), String> {
        match target_path.prefix {
            PathPrefix::Event => self.value.insert(&target_path.path, value),
            PathPrefix::Metadata => self.metadata.insert(&target_path.path, value),
        };
        Ok(())
    }

    fn target_get(&self, target_path: &OwnedTargetPath) -> Result<Option<&Value>, String> {
        let value = match target_path.prefix {
            PathPrefix::Event => self.value.get(&target_path.path),
            PathPrefix::Metadata => self.metadata.get(&target_path.path),
        };
        Ok(value)
    }

    fn target_get_mut(
        &mut self,
        target_path: &OwnedTargetPath,
    ) -> Result<Option<&mut Value>, String> {
        let value = match target_path.prefix {
            PathPrefix::Event => self.value.get_mut(&target_path.path),
            PathPrefix::Metadata => self.metadata.get_mut(&target_path.path),
        };
        Ok(value)
    }

    fn target_remove(
        &mut self,
        target_path: &OwnedTargetPath,
        compact: bool,
    ) -> Result<Option<Value>, String> {
        let prev_value = match target_path.prefix {
            PathPrefix::Event => self.value.remove(&target_path.path, compact),
            PathPrefix::Metadata => self.metadata.remove(&target_path.path, compact),
        };
        Ok(prev_value)
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
        self.get(key).map(AsRef::as_ref)
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
    use super::{SecretTarget, Target, Value};
    use lookup::{OwnedTargetPath, PathPrefix};

    impl Target for Value {
        fn target_insert(
            &mut self,
            target_path: &OwnedTargetPath,
            value: Value,
        ) -> Result<(), String> {
            match target_path.prefix {
                PathPrefix::Event => self.insert(&target_path.path, value),
                PathPrefix::Metadata => panic!("Value has no metadata. Use `TargetValue` instead."),
            };
            Ok(())
        }

        fn target_get(&self, target_path: &OwnedTargetPath) -> Result<Option<&Value>, String> {
            match target_path.prefix {
                PathPrefix::Event => Ok(self.get(&target_path.path)),
                PathPrefix::Metadata => panic!("Value has no metadata. Use `TargetValue` instead."),
            }
        }

        fn target_get_mut(
            &mut self,
            target_path: &OwnedTargetPath,
        ) -> Result<Option<&mut Value>, String> {
            match target_path.prefix {
                PathPrefix::Event => Ok(self.get_mut(&target_path.path)),
                PathPrefix::Metadata => panic!("Value has no metadata. Use `TargetValue` instead."),
            }
        }

        fn target_remove(
            &mut self,
            target_path: &OwnedTargetPath,
            compact: bool,
        ) -> Result<Option<Value>, String> {
            match target_path.prefix {
                PathPrefix::Event => Ok(self.remove(&target_path.path, compact)),
                PathPrefix::Metadata => panic!("Value has no metadata. Use `TargetValue` instead."),
            }
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

    use lookup::owned_value_path;

    use super::*;
    use crate::value;

    #[test]
    fn target_get() {
        let cases = vec![
            (value!(true), owned_value_path!(), Ok(Some(value!(true)))),
            (value!(true), owned_value_path!("foo"), Ok(None)),
            (value!({}), owned_value_path!(), Ok(Some(value!({})))),
            (
                value!({foo: "bar"}),
                owned_value_path!(),
                Ok(Some(value!({foo: "bar"}))),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("foo"),
                Ok(Some(value!("bar"))),
            ),
            (value!({foo: "bar"}), owned_value_path!("bar"), Ok(None)),
            (
                value!([1, 2, 3, 4, 5]),
                owned_value_path!(1),
                Ok(Some(value!(2))),
            ),
            (
                value!({foo: [{bar: true}]}),
                owned_value_path!("foo", 0, "bar"),
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2}}}),
                owned_value_path!("foo", vec!["qux", r#"bar baz"#], "baz"),
                Ok(Some(value!(2))),
            ),
        ];

        for (value, path, expect) in cases {
            let value: Value = value;
            let target = TargetValue {
                value,
                metadata: value!({}),
                secrets: Secrets::new(),
            };
            let path = OwnedTargetPath::event(path);

            assert_eq!(
                target.target_get(&path).map(Option::<&Value>::cloned),
                expect
            );
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn target_insert() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                owned_value_path!(),
                value!({baz: "qux"}),
                value!({baz: "qux"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("baz"),
                true.into(),
                value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                value!({foo: [{bar: "baz"}]}),
                owned_value_path!("foo", 0, "baz"),
                true.into(),
                value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                value!({foo: {bar: "baz"}}),
                owned_value_path!("bar", "baz"),
                true.into(),
                value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("foo"),
                "baz".into(),
                value!({foo: "baz"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("foo", 2, r#"bar baz"#, "a", "b"),
                true.into(),
                value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1, 2]}),
                owned_value_path!("foo", 5),
                "baz".into(),
                value!({foo: [0, 1, 2, null, null, "baz"]}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("foo", 0),
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: []}),
                owned_value_path!("foo", 0),
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0]}),
                owned_value_path!("foo", 0),
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                owned_value_path!("foo", 0),
                "baz".into(),
                value!({foo: ["baz", 1]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                owned_value_path!("foo", 1),
                "baz".into(),
                value!({foo: [0, "baz"]}),
                Ok(()),
            ),
        ];

        for (target, path, value, expect, result) in cases {
            let mut target = TargetValue {
                value: target,
                metadata: value!({}),
                secrets: Secrets::new(),
            };
            let path = OwnedTargetPath::event(path);

            assert_eq!(
                Target::target_insert(&mut target, &path, value.clone()),
                result
            );
            assert_eq!(target.value, expect);
            assert_eq!(
                Target::target_get(&target, &path).map(Option::<&Value>::cloned),
                Ok(Some(value))
            );
        }
    }

    #[test]
    fn target_remove() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                owned_value_path!("baz"),
                false,
                None,
                Some(value!({foo: "bar"})),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!("foo"),
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar"}),
                owned_value_path!(vec![r#"foo bar"#, "foo"]),
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                owned_value_path!(),
                false,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                owned_value_path!(),
                true,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: [0]}),
                owned_value_path!("foo", 0),
                false,
                Some(value!(0)),
                Some(value!({foo: []})),
            ),
            (
                value!({foo: [0]}),
                owned_value_path!("foo", 0),
                true,
                Some(value!(0)),
                Some(value!({})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                owned_value_path!("foo", r#"bar baz"#, 0),
                false,
                Some(value!(0)),
                Some(value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                owned_value_path!("foo", r#"bar baz"#, 0),
                true,
                Some(value!(0)),
                Some(value!({bar: "baz"})),
            ),
        ];

        for (target, path, compact, value, expect) in cases {
            let path = OwnedTargetPath::event(path);

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
                Target::target_get(&target, &OwnedTargetPath::event_root())
                    .map(Option::<&Value>::cloned),
                Ok(expect)
            );
        }
    }
}
