use std::{collections::BTreeMap, iter::Peekable};

use crate::{SharedValue, Value};
use lookup::{FieldBuf, SegmentBuf};

impl Value {
    pub(crate) fn remove_by_segment(&mut self, segment: &SegmentBuf) {
        match segment {
            SegmentBuf::Field(FieldBuf { name, .. }) => self
                .as_object_mut()
                .and_then(|map| map.remove(name.as_str())),

            SegmentBuf::Coalesce(fields) => fields
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

    pub(crate) fn update_by_segments<'a, T>(&mut self, mut segments: Peekable<T>, new: SharedValue)
    where
        T: Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let segment = match segments.next() {
            Some(segments) => segments,
            None => return,
        };

        let mut handle_field = |field: &str, new, mut segments: Peekable<T>| {
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

            match segments.peek() {
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
                    SegmentBuf::Index(_) => {
                        map.insert(key, SharedValue::from(Value::Array(vec![])))
                    }
                    _ => map.insert(key, SharedValue::from(Value::from(BTreeMap::default()))),
                },
            };
            map.get_mut(field)
                .unwrap()
                .clone()
                .insert_by_segments(segments, new);
        };

        match segment {
            SegmentBuf::Field(FieldBuf { name, .. }) => handle_field(name, new, segments),

            SegmentBuf::Coalesce(fields) => {
                // At this point, we know that the coalesced field query did not
                // result in an actual value, so none of the fields match an
                // existing field. We'll pick the last field in the list to
                // insert the new value into.
                let field = match fields.last() {
                    Some(field) => field,
                    None => return,
                };

                handle_field(field.as_str(), new, segments)
            }
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
                        array.insert(0, SharedValue::from(Value::Null))
                    }

                    match segments.peek() {
                        None => {
                            array.insert(0, new);
                            return;
                        }
                        Some(next) => match next {
                            SegmentBuf::Index(_) => {
                                array.insert(0, SharedValue::from(Value::Array(vec![])))
                            }
                            _ => {
                                array.insert(0, SharedValue::from(Value::from(BTreeMap::default())))
                            }
                        },
                    };

                    array
                        .first_mut()
                        .expect("exists")
                        .clone()
                        .insert_by_segments(segments, new);
                } else {
                    let index = index as usize;

                    // right-padded with null values
                    if array.len() < index {
                        array.resize(index, SharedValue::from(Value::Null));
                    }

                    match segments.peek() {
                        None => {
                            array.push(new);
                            return;
                        }
                        Some(next) => match next {
                            SegmentBuf::Index(_) => {
                                array.push(SharedValue::from(Value::Array(vec![])))
                            }
                            _ => array.push(SharedValue::from(Value::from(BTreeMap::default()))),
                        },
                    }

                    array
                        .last_mut()
                        .expect("exists")
                        .clone()
                        .insert_by_segments(segments, new);
                }
            }
        }
    }
}
