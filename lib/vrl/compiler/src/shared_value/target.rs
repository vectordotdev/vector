use crate::{SharedValue, Target, Value};
use lookup::{FieldBuf, LookupBuf, SegmentBuf};
use std::collections::BTreeMap;
use std::iter::Peekable;

impl Target for SharedValue {
    fn insert(&mut self, path: &LookupBuf, value: SharedValue) -> Result<(), String> {
        self.clone().insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &LookupBuf) -> Result<Option<SharedValue>, String> {
        Ok(self.clone().get_by_path(path))
    }

    fn remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<SharedValue>, String> {
        let value = self.get(path)?;
        self.clone().remove_by_path(path, compact);

        Ok(value)
    }
}

impl SharedValue {
    /// Get a reference to a value from a given path.
    ///
    /// # Examples
    ///
    /// Given an existing value, there are three routes this function can take:
    ///
    /// 1. If the path points to the root (`.`), it will return the current
    ///    value:
    ///
    ///    ```rust
    ///    # use vrl_compiler::Value;
    ///    # use lookup::LookupBuf;
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Boolean(true);
    ///    let path = LookupBuf::root();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 2. If the path points to an index, if the value is an `Array`, it will
    ///    return the value at the given index, if one exists, or it will return
    ///    `None`:
    ///
    ///    ```rust
    ///    # use vrl_compiler::Value;
    ///    # use lookup::LookupBuf;
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Array(vec![false.into(), true.into()]);
    ///    let path = LookupBuf::from_str("[1]").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 3. If the path points to a nested path, if the value is an `Object`, it will
    ///    traverse into the map, and return the appropriate value, if one
    ///    exists:
    ///
    ///    ```rust
    ///    # use vrl_compiler::Value;
    ///    # use lookup::LookupBuf;
    ///    # use std::str::FromStr;
    ///    # use std::collections::BTreeMap;
    ///    # use std::iter::FromIterator;
    ///
    ///    let map = BTreeMap::from_iter(vec![("foo".to_owned(), true.into())].into_iter());
    ///    let value = Value::Object(map);
    ///    let path = LookupBuf::from_str("foo").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    pub fn get_by_path(self, path: &LookupBuf) -> Option<SharedValue> {
        self.get_by_segments(path.as_segments().iter())
    }

    /*
    /// Similar to [`Value::get_by_path`], but returns a mutable reference to
    /// the value.
    pub fn get_by_path_mut(&mut self, path: &LookupBuf) -> Option<&mut Value> {
        self.get_by_segments_mut(path.as_segments().iter())
    }
    */

    /// Insert a value, given the provided path.
    ///
    /// # Examples
    ///
    /// ## Insert At Field
    ///
    /// ```
    /// # use vrl_compiler::Value;
    /// # use lookup::LookupBuf;
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let fields = vec![("foo".to_owned(), Value::from("bar"))];
    /// let map = BTreeMap::from_iter(fields.into_iter());
    ///
    /// let mut value = Value::Object(map);
    /// let path = LookupBuf::from_str(".foo").unwrap();
    ///
    /// value.insert_by_path(&path, true.into());
    ///
    /// assert_eq!(
    ///     value.get_by_path(&path),
    ///     Some(&true.into()),
    /// )
    /// ```
    ///
    /// ## Insert Into Array
    ///
    /// ```
    /// # use vrl_compiler::{value, Value};
    /// # use lookup::LookupBuf;
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let mut value = value!([false, true]);
    /// let path = LookupBuf::from_str("[1].foo").unwrap();
    ///
    /// value.insert_by_path(&path, "bar".into());
    ///
    /// assert_eq!(
    ///     value.get_by_path(&LookupBuf::root()),
    ///     Some(&value!([false, {foo: "bar"}])),
    /// )
    /// ```
    ///
    pub fn insert_by_path(self, path: &LookupBuf, new: SharedValue) {
        self.insert_by_segments(path.as_segments().iter().peekable(), new)
    }

    /// Remove a value, given the provided path.
    ///
    /// This works similar to [`Value::get_by_path`], except that it removes the
    /// value at the provided path, instead of returning it.
    ///
    /// The one difference is if a root path (`.`) is provided. In this case,
    /// the [`Value`] object (i.e. "self") is set to `Value::Null`.
    ///
    /// If the `compact` argument is set to `true`, then any `Array` or `Object`
    /// that had one of its elements removed and is now empty, is removed as
    /// well.
    pub fn remove_by_path(self, path: &LookupBuf, compact: bool) {
        self.remove_by_segments(path.as_segments().iter().peekable(), compact)
    }

    fn get_by_segments<'a, T>(self, mut segments: T) -> Option<SharedValue>
    where
        T: Iterator<Item = &'a SegmentBuf>,
    {
        let segment = match segments.next() {
            Some(segment) => segment,
            None => return Some(self),
        };

        self.get_by_segment(segment)
            .and_then(|value| value.get_by_segments(segments))
    }

    fn get_by_segment(self, segment: &SegmentBuf) -> Option<SharedValue> {
        match segment {
            SegmentBuf::Field(FieldBuf { name, .. }) => self
                .borrow()
                .as_object()
                .and_then(|map| map.get(name.as_str()))
                .cloned(),
            SegmentBuf::Coalesce(fields) => self
                .borrow()
                .as_object()
                .and_then(|map| fields.iter().find_map(|field| map.get(field.as_str())))
                .cloned(),
            SegmentBuf::Index(index) => self.borrow().as_array().and_then(|array| {
                let len = array.len() as isize;
                if *index >= len || index.abs() > len {
                    return None;
                }

                index
                    .checked_rem_euclid(len)
                    .and_then(|i| array.get(i as usize))
                    .cloned()
            }),
        }
    }

    /*
     * We don't need _mut anymore sinc Rc<RefCell> is runtime mutable.
    fn get_by_segments_mut<'a, T>(&mut self, mut segments: T) -> Option<Rc<RefCell<Value>>>
    where
        T: Iterator<Item = &'a SegmentBuf>,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment_mut(segment)
            .and_then(|value| value.borrow_mut().get_by_segments_mut(segments).clone())
    }

    fn get_by_segment_mut(&mut self, segment: &SegmentBuf) -> Option<Rc<RefCell<Value>>> {
        match segment {
            SegmentBuf::Field(FieldBuf { name, .. }) => self
                .as_object_mut()
                .and_then(|map| map.get_mut(name.as_str()))
                .cloned(),
            SegmentBuf::Coalesce(fields) => self.as_object_mut().and_then(|map| {
                fields
                    .iter()
                    .find(|field| map.contains_key(field.as_str()))
                    .and_then(move |field| map.get_mut(field.as_str()))
                    .cloned()
            }),
            SegmentBuf::Index(index) => self.as_array_mut().and_then(|array| {
                let len = array.len() as isize;
                if *index >= len || index.abs() > len {
                    return None;
                }

                index
                    .checked_rem_euclid(len)
                    .and_then(move |i| array.get_mut(i as usize))
                    .cloned()
            }),
        }
    }
    */

    fn remove_by_segments<'a, T>(self, mut segments: Peekable<T>, compact: bool)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => {
                return match &mut *self.borrow_mut() {
                    Value::Object(v) => v.clear(),
                    Value::Array(v) => v.clear(),
                    _ => {
                        // TODO This needs serious testing.
                        self.replace(Value::Null);
                    }
                };
            }
        };

        if segments.peek().is_none() {
            return self.borrow_mut().remove_by_segment(segment);
        }

        if let Some(value) = self.clone().get_by_segment(segment) {
            value.clone().remove_by_segments(segments, compact);

            // TODO This needs serious testing.
            match &*value.borrow() {
                Value::Object(v) if compact & v.is_empty() => {
                    self.borrow_mut().remove_by_segment(segment)
                }
                Value::Array(v) if compact & v.is_empty() => {
                    self.borrow_mut().remove_by_segment(segment)
                }
                _ => {}
            }
        }
    }

    pub fn insert_by_segments<'a, T>(self, mut segments: Peekable<T>, new: SharedValue)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let segment = match segments.peek() {
            Some(segment) => segment,
            None => {
                // TODO, A swap may be the wrong thing here.
                self.swap(&new); // return *self = new,
                return;
            }
        };

        // As long as the provided segments match the shape of the value, we'll
        // traverse down the tree. Once we encounter a value kind that does not
        // match the requested segment, we'll update the value to match and
        // continue on, until we're able to assign the final `new` value.
        match self.clone().get_by_segment(segment) {
            Some(value) => {
                // We have already consumed this element via a peek.
                let _ = segments.next();
                value.insert_by_segments(segments, new)
            }
            None => self.borrow_mut().update_by_segments(segments, new),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_value;

    #[test]
    fn target_get() {
        let cases = vec![
            (shared_value!(true), vec![], Ok(Some(shared_value!(true)))),
            (shared_value!(true), vec![SegmentBuf::from("foo")], Ok(None)),
            (shared_value!({}), vec![], Ok(Some(shared_value!({})))),
            (
                shared_value!({foo: "bar"}),
                vec![],
                Ok(Some(shared_value!({foo: "bar"}))),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                Ok(Some(shared_value!("bar"))),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("bar")],
                Ok(None),
            ),
            (
                shared_value!([1, 2, 3, 4, 5]),
                vec![SegmentBuf::from(1)],
                Ok(Some(shared_value!(2))),
            ),
            (
                shared_value!({foo: [{bar: true}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("bar"),
                ],
                Ok(Some(shared_value!(true))),
            ),
            (
                shared_value!({foo: {"bar baz": {baz: 2}}}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(vec![FieldBuf::from("qux"), FieldBuf::from(r#""bar baz""#)]),
                    SegmentBuf::from("baz"),
                ],
                Ok(Some(shared_value!(2))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: SharedValue = value;
            let path = LookupBuf::from_segments(segments);

            assert_eq!(value.get(&path), expect);
        }
    }

    #[test]
    fn target_insert() {
        let cases = vec![
            (
                shared_value!({foo: "bar"}),
                vec![],
                shared_value!({baz: "qux"}),
                shared_value!({baz: "qux"}),
                Ok(()),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                true.into(),
                shared_value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                shared_value!({foo: [{bar: "baz"}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("baz"),
                ],
                true.into(),
                shared_value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                shared_value!({foo: {bar: "baz"}}),
                vec![SegmentBuf::from("bar"), SegmentBuf::from("baz")],
                true.into(),
                shared_value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                "baz".into(),
                shared_value!({foo: "baz"}),
                Ok(()),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(2),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from("a"),
                    SegmentBuf::from("b"),
                ],
                true.into(),
                shared_value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
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

        for (mut target, segments, value, expect, result) in cases {
            println!("Inserting at {:?}", segments);
            let path = LookupBuf::from_segments(segments);

            assert_eq!(Target::insert(&mut target, &path, value.clone()), result);
            assert_eq!(target, expect);
            assert_eq!(Target::get(&target, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn target_remove() {
        let cases = vec![
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                false,
                None,
                Some(shared_value!({foo: "bar"})),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                false,
                Some(shared_value!("bar")),
                Some(shared_value!({})),
            ),
            (
                shared_value!({foo: "bar"}),
                vec![SegmentBuf::coalesce(vec![
                    FieldBuf::from(r#""foo bar""#),
                    FieldBuf::from("foo"),
                ])],
                false,
                Some(shared_value!("bar")),
                Some(shared_value!({})),
            ),
            (
                shared_value!({foo: "bar", baz: "qux"}),
                vec![],
                false,
                Some(shared_value!({foo: "bar", baz: "qux"})),
                Some(shared_value!({})),
            ),
            (
                shared_value!({foo: "bar", baz: "qux"}),
                vec![],
                true,
                Some(shared_value!({foo: "bar", baz: "qux"})),
                Some(shared_value!({})),
            ),
            (
                shared_value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                false,
                Some(shared_value!(0)),
                Some(shared_value!({foo: []})),
            ),
            (
                shared_value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                true,
                Some(shared_value!(0)),
                Some(shared_value!({})),
            ),
            (
                shared_value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                false,
                Some(shared_value!(0)),
                Some(shared_value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                shared_value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                true,
                Some(shared_value!(0)),
                Some(shared_value!({bar: "baz"})),
            ),
        ];

        for (mut target, segments, compact, value, expect) in cases {
            let path = LookupBuf::from_segments(segments);

            assert_eq!(Target::remove(&mut target, &path, compact), Ok(value));
            assert_eq!(Target::get(&target, &LookupBuf::root()), Ok(expect));
        }
    }
}
