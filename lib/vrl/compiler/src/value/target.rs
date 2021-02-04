use crate::{
    path::{Field, Segment, Segment::*},
    Path, Target, Value,
};
use std::collections::BTreeMap;

impl Target for Value {
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &Path) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn remove(&mut self, path: &Path, compact: bool) -> Result<Option<Value>, String> {
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
    pub fn get_by_path(&self, path: &Path) -> Option<&Value> {
        self.get_by_segments(path.segments())
    }

    /// Similar to [`Value::get_by_path`], but returns a mutable reference to
    /// the value.
    pub fn get_by_path_mut(&mut self, path: &Path) -> Option<&mut Value> {
        self.get_by_segments_mut(path.segments())
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
    pub fn insert_by_path(&mut self, path: &Path, new: Value) {
        self.insert_by_segments(path.segments(), new)
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
    pub fn remove_by_path(&mut self, path: &Path, compact: bool) {
        self.remove_by_segments(path.segments(), compact)
    }

    fn get_by_segments(&self, segments: &[Segment]) -> Option<&Value> {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment(segment)
            .and_then(|value| value.get_by_segments(next))
    }

    fn get_by_segment(&self, segment: &Segment) -> Option<&Value> {
        match segment {
            Field(field) => self.as_object().and_then(|map| map.get(field.as_str())),
            Coalesce(fields) => self
                .as_object()
                .and_then(|map| fields.iter().find_map(|field| map.get(field.as_str()))),
            Index(index) => self.as_array().and_then(|array| {
                let index = *index;

                if index < 0 {
                    let index = index % array.len() as i64;
                    array.get(index.abs() as usize)
                } else {
                    array.get(index as usize)
                }
            }),
        }
    }

    fn get_by_segments_mut(&mut self, segments: &[Segment]) -> Option<&mut Value> {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => return Some(self),
        };

        self.get_by_segment_mut(segment)
            .and_then(|value| value.get_by_segments_mut(next))
    }

    fn get_by_segment_mut(&mut self, segment: &Segment) -> Option<&mut Value> {
        match segment {
            Field(field) => self
                .as_object_mut()
                .and_then(|map| map.get_mut(field.as_str())),
            Coalesce(fields) => self.as_object_mut().and_then(|map| {
                fields
                    .iter()
                    .find(|field| map.contains_key(field.as_str()))
                    .and_then(move |field| map.get_mut(field.as_str()))
            }),
            Index(index) => self.as_array_mut().and_then(|array| {
                let index = *index;

                if index < 0 {
                    let len = array.len();
                    let index = index % len as i64;
                    array.get_mut(index.abs() as usize)
                } else {
                    array.get_mut(index as usize)
                }
            }),
        }
    }

    fn remove_by_segments(&mut self, segments: &[Segment], compact: bool) {
        let (segment, next) = match segments.split_first() {
            Some(segments) => segments,
            None => {
                return match self {
                    Value::Object(v) => v.clear(),
                    Value::Array(v) => v.clear(),
                    _ => *self = Value::Null,
                }
            }
        };

        if next.is_empty() {
            return self.remove_by_segment(segment);
        }

        if let Some(value) = self.get_by_segment_mut(segment) {
            value.remove_by_segments(next, compact);

            match value {
                Value::Object(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                Value::Array(v) if compact & v.is_empty() => self.remove_by_segment(segment),
                _ => {}
            }
        }
    }

    fn remove_by_segment(&mut self, segment: &Segment) {
        match segment {
            Field(field) => self
                .as_object_mut()
                .and_then(|map| map.remove(field.as_str())),

            Coalesce(fields) => fields
                .iter()
                .find(|field| {
                    self.as_object()
                        .map(|map| map.contains_key(field.as_str()))
                        .unwrap_or_default()
                })
                .and_then(|field| {
                    self.as_object_mut()
                        .and_then(|map| map.remove(field.as_str()))
                }),

            Index(index) => self.as_array_mut().map(|array| {
                let index = *index;

                if index < 0 {
                    let index = index % array.len() as i64;
                    array.remove(index.abs() as usize)
                } else {
                    array.remove(index as usize)
                }
            }),
        };
    }

    fn insert_by_segments(&mut self, segments: &[Segment], new: Value) {
        let (segment, rest) = match segments.split_first() {
            Some(segments) => segments,
            None => return *self = new,
        };

        // As long as the provided segments match the shape of the value, we'll
        // traverse down the tree. Once we encounter a value kind that does not
        // match the requested segment, we'll update the value to match and
        // continue on, until we're able to assign the final `new` value.
        match self.get_by_segment_mut(segment) {
            Some(value) => value.insert_by_segments(rest, new),
            None => self.update_by_segments(segments, new),
        };
    }

    fn update_by_segments(&mut self, segments: &[Segment], new: Value) {
        let (segment, rest) = match segments.split_first() {
            Some(segments) => segments,
            None => return,
        };

        let mut handle_field = |field: &Field, new| {
            let key = field.as_str().to_owned();

            // `handle_field` is used to update map values, if the current value
            // isn't a map, we need to make it one.
            if !matches!(self, Value::Object(_)) {
                *self = BTreeMap::default().into()
            }

            let map = match self {
                Value::Object(map) => map,
                _ => unreachable!(),
            };

            match rest.first() {
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
                    Index(_) => map.insert(key, Value::Array(vec![])),
                    _ => map.insert(key, BTreeMap::default().into()),
                },
            };

            map.get_mut(field.as_str())
                .unwrap()
                .insert_by_segments(rest, new);
        };

        match segment {
            Field(field) => handle_field(field, new),

            Coalesce(fields) => {
                // At this point, we know that the coalesced field query did not
                // result in an actual value, so none of the fields match an
                // existing field. We'll pick the last field in the list to
                // insert the new value into.
                let field = match fields.last() {
                    Some(field) => field,
                    None => return,
                };

                handle_field(field, new)
            }

            Index(index) => match self {
                // If the current value is an array, we need to either swap out
                // an existing value, or append the new value to the array.
                Value::Array(array) => {
                    let index = *index;

                    // If the array has less items than needed, we'll fill it in
                    // with `Null` values.
                    if index > 0i64 && array.len() < index as usize {
                        array.resize(index as usize, Value::Null);
                    }

                    match rest.first() {
                        None => {
                            array.push(new);
                            return;
                        }
                        Some(next) => match next {
                            Index(_) => array.push(Value::Array(vec![])),
                            _ => array.push(BTreeMap::default().into()),
                        },
                    };

                    array
                        .last_mut()
                        .expect("exists")
                        .insert_by_segments(rest, new);
                }

                // Any non-array value is swapped out with an array.
                _ => {
                    let index = *index;
                    let mut array = Vec::with_capacity(index as usize + 1);
                    array.resize(index as usize, Value::Null);

                    match rest.first() {
                        None => {
                            array.push(new);
                            return *self = array.into();
                        }
                        Some(next) => match next {
                            Index(_) => array.push(Value::Array(vec![])),
                            _ => array.push(BTreeMap::default().into()),
                        },
                    };

                    array
                        .last_mut()
                        .expect("exists")
                        .insert_by_segments(rest, new);

                    *self = array.into();
                }
            },
        }
    }
}

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
