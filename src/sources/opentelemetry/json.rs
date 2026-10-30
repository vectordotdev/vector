//! OTLP JSON is ProtoJSON with hexadecimal trace/span IDs and integer enums.
use std::sync::LazyLock;

use base64::Engine as _;
use bytes::Bytes;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DeserializeOptions, DynamicMessage, Kind, MessageDescriptor};
use serde_json::Value;
use vector_lib::opentelemetry::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    TRACES_REQUEST_MESSAGE_TYPE,
};

use super::config::{LOGS, METRICS, TRACES};

static DESCRIPTORS: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(DESCRIPTOR_BYTES).expect("compiled OTLP descriptors must be valid")
});

fn normalize_value(kind: Kind, is_id: bool, value: &mut Value) -> Result<(), ()> {
    if value.is_null() {
        return Ok(());
    }
    match kind {
        Kind::Message(descriptor) => normalize_message(&descriptor, value),
        Kind::Bytes if is_id => {
            let encoded = value.as_str().ok_or(())?;
            let bytes = hex::decode(encoded).map_err(|_| ())?;
            *value = Value::String(base64::prelude::BASE64_STANDARD.encode(bytes));
            Ok(())
        }
        Kind::Enum(_) if !value.is_number() => Err(()),
        _ => Ok(()),
    }
}

fn normalize_message(descriptor: &MessageDescriptor, value: &mut Value) -> Result<(), ()> {
    let Some(object) = value.as_object_mut() else {
        // The typed deserializer reports wrong message shapes after normalization.
        return Ok(());
    };
    for field in descriptor.fields() {
        let names = [field.json_name(), field.name()];
        for (index, name) in names.into_iter().enumerate() {
            if index == 1 && name == field.json_name() {
                continue;
            }
            let Some(value) = object.get_mut(name) else {
                continue;
            };
            let is_id = matches!(field.name(), "trace_id" | "span_id" | "parent_span_id");
            let kind = field.kind();
            if field.is_list() {
                if let Some(values) = value.as_array_mut() {
                    for value in values {
                        normalize_value(kind.clone(), is_id, value)?;
                    }
                }
            } else if field.is_map() {
                if let (Kind::Message(entry), Some(values)) = (kind, value.as_object_mut()) {
                    let kind = entry.get_field(2).expect("map value descriptor").kind();
                    for value in values.values_mut() {
                        normalize_value(kind.clone(), false, value)?;
                    }
                }
            } else {
                normalize_value(kind, is_id, value)?;
            }
        }
    }
    Ok(())
}

pub(super) fn request(signal: &str, body: Bytes) -> Result<Bytes, ()> {
    let name = match signal {
        TRACES => TRACES_REQUEST_MESSAGE_TYPE,
        LOGS => LOGS_REQUEST_MESSAGE_TYPE,
        METRICS => METRICS_REQUEST_MESSAGE_TYPE,
        _ => unreachable!("only OTLP signal endpoints call this helper"),
    };
    let descriptor = DESCRIPTORS
        .get_message_by_name(name)
        .expect("compiled OTLP request");
    let mut value: Value = serde_json::from_slice(&body).map_err(|_| ())?;
    if !value.is_object() {
        return Err(());
    }
    normalize_message(&descriptor, &mut value)?;
    let options = DeserializeOptions::new().deny_unknown_fields(false);
    let request =
        DynamicMessage::deserialize_with_options(descriptor, value, &options).map_err(|_| ())?;
    Ok(Bytes::from(request.encode_to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;

    #[test]
    fn empty_json_is_decoded_but_absent_or_malformed_json_is_not_empty() {
        for input in [
            r#"{}"#,
            r#"{"resourceSpans":[]}"#,
            r#"{"resourceSpans":[{}]}"#,
            r#"{"resourceSpans":[{"scopeSpans":[{}]}]}"#,
        ] {
            let body = request(TRACES, Bytes::copy_from_slice(input.as_bytes())).unwrap();
            assert!(super::super::empty::protobuf(TRACES, body).unwrap());
        }
        for input in ["", " ", "null", "[]", "{", r#"{"resourceSpans":42}"#] {
            assert!(request(TRACES, Bytes::copy_from_slice(input.as_bytes())).is_err());
        }
    }

    #[test]
    fn json_preserves_ids_scope_resource_and_zero_valued_span() {
        let body = request(TRACES, Bytes::from_static(br#"{
          "resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"test-service"}}]},
            "schemaUrl":"https://example.invalid/resource-schema",
            "scopeSpans":[{"scope":{"name":"test-scope","version":"1"},"schemaUrl":"https://example.invalid/scope-schema",
              "spans":[{"traceId":"0102030405060708090a0b0c0d0e0f10","spanId":"0102030405060708",
                "parentSpanId":"0807060504030201","name":"test","kind":1,
                "startTimeUnixNano":"0","endTimeUnixNano":"0",
                "links":[{"traceId":"10100f0e0d0c0b0a0908070605040302","spanId":"0807060504030201"}]}]}]}]
        }"#)).unwrap();
        let request = ExportTraceServiceRequest::decode(body).unwrap();
        let resource = &request.resource_spans[0];
        assert_eq!(
            resource.schema_url,
            "https://example.invalid/resource-schema"
        );
        let scope = &resource.scope_spans[0];
        assert_eq!(scope.scope.as_ref().unwrap().name, "test-scope");
        assert_eq!(scope.schema_url, "https://example.invalid/scope-schema");
        let span = &scope.spans[0];
        assert_eq!(
            span.trace_id,
            hex::decode("0102030405060708090a0b0c0d0e0f10").unwrap()
        );
        assert_eq!(span.span_id, hex::decode("0102030405060708").unwrap());
        assert_eq!(
            span.parent_span_id,
            hex::decode("0807060504030201").unwrap()
        );
        assert_eq!(
            span.links[0].span_id,
            hex::decode("0807060504030201").unwrap()
        );
        assert_eq!(span.start_time_unix_nano, span.end_time_unix_nano);
        assert!(!super::super::empty::traces(&request));
    }

    #[test]
    fn json_rejects_invalid_hex_and_named_enums_without_echoing_input() {
        for field in [r#""traceId":"not-hex""#, r#""kind":"SPAN_KIND_INTERNAL""#] {
            let body =
                format!(r#"{{"resourceSpans":[{{"scopeSpans":[{{"spans":[{{{field}}}]}}]}}]}}"#);
            assert!(request(TRACES, Bytes::from(body)).is_err());
        }
    }
}
