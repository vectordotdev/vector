use lookup::{FieldBuf, LookupBuf, SegmentBuf};
use snafu::Snafu;
use std::{collections::BTreeMap, fmt::Debug};
use vrl_compiler::Value;

pub fn insert_field(
    source: &mut Value,
    lookup: impl Into<LookupBuf> + Debug,
    value: Value,
) -> std::result::Result<(), Error> {
    let mut working_lookup: LookupBuf = lookup.into();
    let this_segment = working_lookup.pop_front();
    match (this_segment, source) {
        // We've met an end and found our value.
        (None, Value::Object(ref mut target_map)) => {
            if let Value::Object(value_map) = value {
                target_map.extend(value_map);
                Ok(())
            } else {
                Err(Error::InsertionFailed {
                    at: None,
                    original_target: working_lookup,
                })
            }
        }
        (None, item) => {
            let mut value = value;
            core::mem::swap(&mut value, item);
            Ok(())
        }
        // descend into an array
        (Some(SegmentBuf::Index(i)), ref mut item) => {
            let i = i as usize;
            match (item, value) {
                (Value::Object(ref mut target_map), Value::Object(value_map)) => {
                    target_map.extend(value_map);
                }
                (Value::Array(ref mut values), value) => {
                    if i >= values.len() {
                        values.resize(i + 1, Value::Null);
                    }
                    values[i] = value;
                }
                (single_value, value) => {
                    let mut values = Vec::with_capacity(i + 1);
                    if i >= values.len() {
                        values.resize(i + 1, Value::Null);
                    }
                    values[i] = value;
                    let mut value = Value::Array(values);
                    core::mem::swap(&mut value, single_value);
                }
            }
            Ok(())
        }
        (Some(segment), Value::Boolean(_))
        | (Some(segment), Value::Bytes(_))
        | (Some(segment), Value::Float(_))
        | (Some(segment), Value::Integer(_))
        | (Some(segment), Value::Null)
        | (Some(segment), Value::Array(_)) => Err(Error::InsertionFailed {
            at: Some(segment),
            original_target: working_lookup,
        }),
        // Descend into a map
        (Some(SegmentBuf::Field(FieldBuf { ref name, .. })), Value::Object(ref mut map)) => {
            insert_map(name, working_lookup, map, value)
        }
        (Some(segment), _) => Err(Error::InsertionFailed {
            at: Some(segment),
            original_target: working_lookup,
        }),
    }
}

fn insert_map(
    name: &str,
    mut working_lookup: LookupBuf,
    map: &mut BTreeMap<String, Value>,
    value: Value,
) -> Result<(), Error> {
    match working_lookup.get(0) {
        Some(_) => insert_field(
            map.entry(name.to_string())
                .or_insert_with(|| Value::Object(Default::default())),
            working_lookup,
            value,
        ),
        None => {
            map.insert(name.to_string(), value);
            Ok(())
        }
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display(
    "Cannot insert nested value at {:?}. {} was the original target.",// TODO
    at,
    original_target
    ))]
    InsertionFailed {
        at: Option<SegmentBuf>,
        original_target: LookupBuf,
    },
    #[snafu(display("Lookup Error at: {}", lookup))]
    LookupFailed { lookup: LookupBuf },
}
