//! Recursive `AnyValue` <-> `AttrValue` conversion.

use std::collections::btree_map::Entry;

use bytes::Bytes;
use vector_core::event::typed_trace::{
    AttrMap, AttrValue, Attributes, DroppedCount, TraceConversionIssue, TraceConversionReporter,
};

use crate::proto::common::v1::{
    AnyValue, ArrayValue, KeyValue, KeyValueList, any_value::Value as PbValue,
};

impl AnyValue {
    /// Converts to an [`AttrValue`]. Duplicate keys in nested key-value lists count against
    /// `dropped`, the enclosing item's in-band count.
    fn into_attr(
        self,
        dropped: &mut DroppedCount,
        reporter: &mut impl TraceConversionReporter,
    ) -> AttrValue {
        match self.value {
            None => AttrValue::Null,
            Some(PbValue::StringValue(v)) => AttrValue::String(v),
            Some(PbValue::BytesValue(v)) => AttrValue::Bytes(Bytes::from(v)),
            Some(PbValue::BoolValue(v)) => AttrValue::Bool(v),
            Some(PbValue::IntValue(v)) => AttrValue::Integer(v),
            Some(PbValue::DoubleValue(v)) => AttrValue::Float(v),
            Some(PbValue::ArrayValue(arr)) => AttrValue::Array(
                arr.values
                    .into_iter()
                    .map(|item| item.into_attr(dropped, reporter))
                    .collect(),
            ),
            Some(PbValue::KvlistValue(list)) => {
                AttrValue::Map(key_values_to_attr_map(list.values, dropped, reporter))
            }
        }
    }
}

pub fn key_values_to_attributes(
    kvs: Vec<KeyValue>,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> Attributes {
    Attributes::from(key_values_to_attr_map(kvs, dropped, reporter))
}

pub fn key_values_to_attr_map(
    kvs: Vec<KeyValue>,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) -> AttrMap {
    let mut map = AttrMap::new();
    for kv in kvs {
        let value = kv
            .value
            .map_or(AttrValue::Null, |value| value.into_attr(dropped, reporter));
        match map.entry(kv.key.into()) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(mut entry) => {
                dropped.increment(1);
                reporter.report(TraceConversionIssue::DuplicateAttribute { key: entry.key() });
                entry.insert(value);
            }
        }
    }
    map
}

impl From<AttrValue> for AnyValue {
    fn from(value: AttrValue) -> Self {
        Self {
            value: match value {
                AttrValue::Null => None,
                AttrValue::String(v) => Some(PbValue::StringValue(v)),
                AttrValue::Bytes(v) => Some(PbValue::BytesValue(v.to_vec())),
                AttrValue::Bool(v) => Some(PbValue::BoolValue(v)),
                AttrValue::Integer(v) => Some(PbValue::IntValue(v)),
                AttrValue::Float(v) => Some(PbValue::DoubleValue(v)),
                AttrValue::Array(values) => Some(PbValue::ArrayValue(ArrayValue {
                    values: values.into_iter().map(Self::from).collect(),
                })),
                AttrValue::Map(map) => Some(PbValue::KvlistValue(KeyValueList {
                    values: attr_map_to_key_values(map),
                })),
            },
        }
    }
}

pub fn attributes_to_key_values(attributes: Attributes) -> Vec<KeyValue> {
    attr_map_to_key_values(attributes.into_inner())
}

pub fn attr_map_to_key_values(map: AttrMap) -> Vec<KeyValue> {
    map.into_iter()
        .map(|(key, value)| KeyValue {
            key: key.into(),
            value: Some(value.into()),
        })
        .collect()
}
