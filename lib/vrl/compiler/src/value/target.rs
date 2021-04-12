use crate::{Target, Value};
use lookup::{LookupBuf, SegmentBuf};
use std::collections::BTreeMap;

impl Target for Value {
    fn insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &LookupBuf) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String> {
        let value = self.get(path)?;
        self.remove_by_path(path, compact);

        Ok(value)
    }
}

impl Value {
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
    ///    # use vrl_compiler::{Path, Value};
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Boolean(true);
    ///    let path = Path::from_str(".").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 2. If the path points to an index, if the value is an `Array`, it will
    ///    return the value at the given index, if one exists, or it will return
    ///    `None`:
    ///
    ///    ```rust
    ///    # use vrl_compiler::{Path, Value};
    ///    # use std::str::FromStr;
    ///
    ///    let value = Value::Array(vec![false.into(), true.into()]);
    ///    let path = Path::from_str(".[1]").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    /// 3. If the path points to a nested path, if the value is an `Object`, it will
    ///    traverse into the map, and return the appropriate value, if one
    ///    exists:
    ///
    ///    ```rust
    ///    # use vrl_compiler::{Path, Value};
    ///    # use std::str::FromStr;
    ///    # use std::collections::BTreeMap;
    ///    # use std::iter::FromIterator;
    ///
    ///    let map = BTreeMap::from_iter(vec![("foo".to_owned(), true.into())].into_iter());
    ///    let value = Value::Object(map);
    ///    let path = Path::from_str(".foo").unwrap();
    ///
    ///    assert_eq!(value.get_by_path(&path), Some(&Value::Boolean(true)))
    ///    ```
    ///
    pub fn get_by_path(&self, path: &LookupBuf) -> Option<&Value> {
        self.get_by_segments(path.as_segments().iter())
    }

    /// Similar to [`Value::get_by_path`], but returns a mutable reference to
    /// the value.
    pub fn get_by_path_mut(&mut self, path: &LookupBuf) -> Option<&mut Value> {
        self.get_by_segments_mut(path.as_segments().iter())
    }

    /// Insert a value, given the provided path.
    ///
    /// # Examples
    ///
    /// ## Insert At Field
    ///
    /// ```
    /// # use vrl_compiler::{Path, Value};
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let fields = vec![("foo".to_owned(), Value::from("bar"))];
    /// let map = BTreeMap::from_iter(fields.into_iter());
    ///
    /// let mut value = Value::Object(map);
    /// let path = Path::from_str(".foo").unwrap();
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
    /// # use vrl_compiler::{value, Path, Value};
    /// # use std::str::FromStr;
    /// # use std::collections::BTreeMap;
    /// # use std::iter::FromIterator;
    ///
    /// let mut value = value!([false, true]);
    /// let path = Path::from_str(".[1].foo").unwrap();
    ///
    /// value.insert_by_path(&path, "bar".into());
    ///
    /// assert_eq!(
    ///     value.get_by_path(&Path::from_str(".").unwrap()),
    ///     Some(&value!([false, {foo: "bar"}])),
    /// )
    /// ```
    ///
    pub fn insert_by_path(&mut self, path: &LookupBuf, new: Value) {
        self.insert_by_segments(path.as_segments().iter(), new)
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
    pub fn remove_by_path(&mut self, path: &LookupBuf, compact: bool) {
        self.remove_by_segments(path.as_segments().iter(), compact)
    }

    fn get_by_segments<'a, T>(&self, mut segments: T) -> Option<&Value>
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

    fn get_by_segment(&self, segment: &SegmentBuf) -> Option<&Value> {
        match segment {
            SegmentBuf::Field { name, .. } => {
                self.as_object().and_then(|map| map.get(name.as_str()))
            }
            SegmentBuf::Coalesce(fields) => todo!(), /*self
            .as_object()
            .and_then(|map| fields.iter().find_map(|field| map.get(field.as_str()))),
             */
            SegmentBuf::Index(index) => self.as_array().and_then(|array| {
                let len = array.len() as isize;
                if *index >= len || index.abs() > len {
                    return None;
                }

                index
                    .checked_rem_euclid(len)
                    .and_then(|i| array.get(i as usize))
            }),
        }
    }

    fn get_by_segments_mut<'a, T>(&mut self, mut segments: T) -> Option<&mut Value>
    where
        T: Iterator<Item = &'a SegmentBuf>,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment_mut(segment)
            .and_then(|value| value.get_by_segments_mut(segments))
    }

    fn get_by_segment_mut(&mut self, segment: &SegmentBuf) -> Option<&mut Value> {
        match segment {
            SegmentBuf::Field { name, .. } => self
                .as_object_mut()
                .and_then(|map| map.get_mut(name.as_str())),
            SegmentBuf::Coalesce(fields) => todo!(), /*self.as_object_mut().and_then(|map| {
            fields
            .iter()
            .find(|field| map.contains_key(field.as_str()))
            .and_then(move |field| map.get_mut(field.as_str()))
            }),*/
            SegmentBuf::Index(index) => self.as_array_mut().and_then(|array| {
                let len = array.len() as isize;
                if *index >= len || index.abs() > len {
                    return None;
                }

                index
                    .checked_rem_euclid(len)
                    .and_then(move |i| array.get_mut(i as usize))
            }),
        }
    }

    fn remove_by_segments<'a, T>(&mut self, mut segments: T, compact: bool)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => {
                return match self {
                    Value::Object(v) => v.clear(),
                    Value::Array(v) => v.clear(),
                    _ => *self = Value::Null,
                }
            }
        };

        let mut peekable = segments.clone().peekable();

        if peekable.peek().is_none() {
            return self.remove_by_segment(segment);
        }

        if let Some(value) = self.get_by_segment_mut(segment) {
            value.remove_by_segments(segments, compact);

            match value {
                Value::Object(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                Value::Array(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                _ => {}
            }
        }
    }

    fn remove_by_segment(&mut self, segment: &SegmentBuf) {
        match segment {
            SegmentBuf::Field { name, .. } => self
                .as_object_mut()
                .and_then(|map| map.remove(name.as_str())),

            SegmentBuf::Coalesce(fields) => todo!(), /*fields
            .iter()
            .find(|field| {
            self.as_object()
            .map(|map| map.contains_key(field.as_str()))
            .unwrap_or_default()
            })
            .and_then(|field| {
            self.as_object_mut()
            .and_then(|map| map.remove(field.as_str()))
            }),*/
            SegmentBuf::Index(index) => self.as_array_mut().and_then(|array| {
                let len = array.len() as isize;
                if *index >= len || index.abs() > len {
                    return None;
                }

                index
                    .checked_rem_euclid(len)
                    .map(|i| array.remove(i as usize))
            }),
        };
    }

    fn insert_by_segments<'a, T>(&mut self, mut segments: T, new: Value)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let original = segments.clone();
        let segment = match segments.next() {
            Some(segments) => segments,
            None => return *self = new,
        };

        // As long as the provided segments match the shape of the value, we'll
        // traverse down the tree. Once we encounter a value kind that does not
        // match the requested segment, we'll update the value to match and
        // continue on, until we're able to assign the final `new` value.
        match self.get_by_segment_mut(segment) {
            Some(value) => value.insert_by_segments(segments, new),
            None => self.update_by_segments(original, new),
        };
    }

    fn update_by_segments<'a, T>(&mut self, mut segments: T, new: Value)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => return,
        };

        let mut handle_field = |field: &str, new, mut segments: T| {
            let key = field.to_owned();

            // `handle_field` is used to update map values, if the current value
            // isn't a map, we need to make it one.
            if !matches!(self, Value::Object(_)) {
                *self = BTreeMap::default().into()
            }

            let map = match self {
                Value::Object(map) => map,
                _ => unreachable!("see invariant above"),
            };

            match segments.next() {
                // If there are no other segments to traverse, we'll add the new
                // value to the current map.
                None => {
                    map.insert(key, new);
                    return;
                }
                // If there are more segments to traverse, insert an empty map
                // or array depending on what the next segment is, and continue
                // to add the next segment.
                Some(next) => match next {
                    SegmentBuf::Index(_) => map.insert(key, Value::Array(vec![])),
                    _ => map.insert(key, BTreeMap::default().into()),
                },
            };

            // TODO: This may be wrong as segments should probably have been peeked
            // in the above segments.next() call.
            map.get_mut(field)
                .unwrap()
                .insert_by_segments(segments, new);
        };

        match segment {
            SegmentBuf::Field { name, .. } => handle_field(name, new, segments),

            SegmentBuf::Coalesce(fields) => todo!(), /*{
            // At this point, we know that the coalesced field query did not
            // result in an actual value, so none of the fields match an
            // existing field. We'll pick the last field in the list to
            // insert the new value into.
            let field = match fields.last() {
            Some(field) => field,
            None => return,
            };

            handle_field(field, new)
            }*/
            SegmentBuf::Index(index) => {
                let array = match self {
                    Value::Array(array) => array,
                    _ => {
                        *self = Value::Array(vec![]);
                        self.as_array_mut().unwrap()
                    }
                };

                let index = *index;

                // If we're dealing with a negative index, we either need to
                // replace an existing value, or insert to the front of the
                // array.
                if index.is_negative() {
                    let abs = index.abs() as usize;

                    // left-padded with null values
                    for _ in 1..abs - array.len() {
                        array.insert(0, Value::Null)
                    }

                    // TODO should this be a peek?
                    match segments.next() {
                        None => {
                            array.insert(0, new);
                            return;
                        }
                        Some(next) => match next {
                            SegmentBuf::Index(_) => array.insert(0, Value::Array(vec![])),
                            _ => array.insert(0, BTreeMap::default().into()),
                        },
                    };

                    array
                        .first_mut()
                        .expect("exists")
                        .insert_by_segments(segments, new);
                } else {
                    let index = index as usize;

                    // right-padded with null values
                    if array.len() < index {
                        array.resize(index, Value::Null);
                    }

                    match segments.next() {
                        None => {
                            array.push(new);
                            return;
                        }
                        Some(next) => match next {
                            SegmentBuf::Index(_) => array.push(Value::Array(vec![])),
                            _ => array.push(BTreeMap::default().into()),
                        },
                    }

                    array
                        .last_mut()
                        .expect("exists")
                        .insert_by_segments(segments, new);
                }
            }
        }
    }
}

/*

TODO We want these back!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{path::Field::*, value};

    #[test]
    fn target_get() {
        let cases = vec![
            (value!(true), vec![], Ok(Some(value!(true)))),
            (
                value!(true),
                vec![Field(Regular("foo".to_string()))],
                Ok(None),
            ),
            (value!({}), vec![], Ok(Some(value!({})))),
            (value!({foo: "bar"}), vec![], Ok(Some(value!({foo: "bar"})))),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                Ok(Some(value!("bar"))),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("bar".to_owned()))],
                Ok(None),
            ),
            (value!([1, 2, 3, 4, 5]), vec![Index(1)], Ok(Some(value!(2)))),
            (
                value!({foo: [{bar: true}]}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("bar".to_owned())),
                ],
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2}}}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Coalesce(vec![
                        Regular("qux".to_owned()),
                        Quoted("bar baz".to_owned()),
                    ]),
                    Field(Regular("baz".to_owned())),
                ],
                Ok(Some(value!(2))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(value.get(&path), expect)
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
                vec![Field(Regular("baz".to_owned()))],
                true.into(),
                value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                value!({foo: [{bar: "baz"}]}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("baz".to_owned())),
                ],
                true.into(),
                value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                value!({foo: {bar: "baz"}}),
                vec![
                    Field(Regular("bar".to_owned())),
                    Field(Regular("baz".to_owned())),
                ],
                true.into(),
                value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                "baz".into(),
                value!({foo: "baz"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(2),
                    Field(Quoted("bar baz".to_owned())),
                    Field(Regular("a".to_owned())),
                    Field(Regular("b".to_owned())),
                ],
                true.into(),
                value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1, 2]}),
                vec![Field(Regular("foo".to_owned())), Index(5)],
                "baz".into(),
                value!({foo: [0, 1, 2, null, null, "baz"]}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: []}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz", 1]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![Field(Regular("foo".to_owned())), Index(1)],
                "baz".into(),
                value!({foo: [0, "baz"]}),
                Ok(()),
            ),
        ];

        for (mut target, segments, value, expect, result) in cases {
            let path = Path::new_unchecked(segments);

            assert_eq!(Target::insert(&mut target, &path, value.clone()), result);
            assert_eq!(target, expect);
            assert_eq!(Target::get(&target, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn target_remove() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![Field(Regular("baz".to_owned()))],
                false,
                None,
                Some(value!({foo: "bar"})),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar"}),
                vec![Coalesce(vec![
                    Quoted("foo bar".to_owned()),
                    Regular("foo".to_owned()),
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
                vec![Field(Regular("foo".to_owned())), Index(0)],
                false,
                Some(value!(0)),
                Some(value!({foo: []})),
            ),
            (
                value!({foo: [0]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                true,
                Some(value!(0)),
                Some(value!({})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                false,
                Some(value!(0)),
                Some(value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                true,
                Some(value!(0)),
                Some(value!({bar: "baz"})),
            ),
        ];

        for (mut target, segments, compact, value, expect) in cases {
            let path = Path::new_unchecked(segments);

            assert_eq!(Target::remove(&mut target, &path, compact), Ok(value));
            assert_eq!(Target::get(&target, &Path::root()), Ok(expect));
        }
    }
}

*/
