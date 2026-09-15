use super::append::read_packed_element;
use super::plan::ScalarKind;
use super::{
    WireToArrowEncoder, WireToArrowError, WireToArrowSerializer, WireToArrowSerializerConfig,
};

use proptest::prelude::*;

use arrow::array::{Array, AsArray};
use arrow::datatypes::{DataType, Field, Fields as ArrowFields, Schema};
use bytes::Bytes;
use prost_reflect::MessageDescriptor;
use prost_reflect::prost::Message as _;
use prost_reflect::prost_types::field_descriptor_proto::{Label, Type as ProtoType};
use prost_reflect::prost_types::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    OneofDescriptorProto,
};
use prost_reflect::{DescriptorPool, DynamicMessage, Value as ProtoValue};
use std::path::PathBuf;
use std::sync::Arc;
use vector_core::event::Event;
use vrl::event_path;
use super::wire::WireValue;

fn encode_varint_for_test(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
    out
}

#[test]
fn packed_element_varint_roundtrip() {
    for v in [0u64, 1, 127, 128, 255, 16384, u32::MAX as u64, u64::MAX] {
        let encoded = encode_varint_for_test(v);
        let (decoded, rest) = read_packed_element(ScalarKind::Int64, &encoded).unwrap();
        assert!(matches!(decoded, WireValue::Varint(d) if d == v), "mismatch on {v}");
        assert!(rest.is_empty(), "buffer not fully consumed for {v}");
    }
}

#[test]
fn packed_element_varint_eof_maps_to_unexpected_eof() {
    assert!(matches!(
        read_packed_element(ScalarKind::Int64, &[0x80u8]),
        Err(WireToArrowError::UnexpectedEof)
    ));
}

#[test]
fn packed_element_varint_overflow_maps_to_varint_overflow() {
    assert!(matches!(
        read_packed_element(ScalarKind::Int64, &[0xffu8; 11]),
        Err(WireToArrowError::VarintOverflow)
    ));
}

#[test]
fn packed_element_fixed32_roundtrip() {
    let bytes = 0x12345678u32.to_le_bytes();
    let (decoded, rest) = read_packed_element(ScalarKind::Fixed32, &bytes).unwrap();
    assert!(matches!(decoded, WireValue::I32(v) if v == 0x12345678));
    assert!(rest.is_empty());
}

#[test]
fn packed_element_fixed64_roundtrip() {
    let v: u64 = 0x0011_2233_4455_6677;
    let bytes = v.to_le_bytes();
    let (decoded, rest) = read_packed_element(ScalarKind::Fixed64, &bytes).unwrap();
    assert!(matches!(decoded, WireValue::I64(d) if d == v));
    assert!(rest.is_empty());
}

fn descriptor_pool(file: &str) -> DescriptorPool {
    let desc_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/protobuf/protos")
        .join(file);
    let bytes = std::fs::read(&desc_path).unwrap();
    DescriptorPool::decode(bytes.as_slice()).unwrap()
}

fn scalar_descriptor() -> MessageDescriptor {
    descriptor_pool("test_protobuf.desc")
        .get_message_by_name("test_protobuf.Person")
        .unwrap()
}

fn rich_descriptor() -> MessageDescriptor {
    descriptor_pool("test_protobuf3.desc")
        .get_message_by_name("test_protobuf3.Person")
        .unwrap()
}

/// Build an ad-hoc `message Bag { repeated int32 numbers = 1; }` descriptor
/// programmatically, since none of the checked-in test protos have a bare
/// repeated scalar field.
fn repeated_int32_descriptor() -> MessageDescriptor {
    let fd = FileDescriptorProto {
        name: Some("wire_to_arrow_test.proto".into()),
        package: Some("wire_to_arrow_test".into()),
        message_type: vec![DescriptorProto {
            name: Some("Bag".into()),
            field: vec![FieldDescriptorProto {
                name: Some("numbers".into()),
                number: Some(1),
                label: Some(Label::Repeated as i32),
                r#type: Some(ProtoType::Int32 as i32),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet { file: vec![fd] };
    let mut bytes = Vec::new();
    set.encode(&mut bytes).unwrap();
    DescriptorPool::decode(bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_to_arrow_test.Bag")
        .unwrap()
}

#[test]
fn scalar_roundtrip() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name("name", ProtoValue::String("Alice".into()));
    msg.set_field_by_name("id", ProtoValue::I32(42));
    msg.set_field_by_name("email", ProtoValue::String("alice@x.com".into()));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 3);
    assert_eq!(batch.column(0).as_string::<i64>().value(0), "Alice");
    assert_eq!(
        batch
            .column(1)
            .as_primitive::<arrow::datatypes::Int32Type>()
            .value(0),
        42
    );
}

#[test]
fn absent_fields_are_null() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Populate only `name`; `id` and `email` should show up as null.
    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name("name", ProtoValue::String("only name".into()));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert!(!batch.column(0).is_null(0));
    assert!(batch.column(1).is_null(0), "id should be null");
    assert!(batch.column(2).is_null(0), "email should be null");
}

#[test]
fn unknown_wire_fields_are_skipped() {
    // Person.phones (field 4 in proto2 test_protobuf.Person) is not in
    // our Arrow schema but will appear in wire bytes when populated. The
    // encoder should skip it cleanly.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let phone_desc = desc
        .get_field_by_name("phones")
        .unwrap()
        .kind()
        .as_message()
        .unwrap()
        .clone();
    let mut phone = DynamicMessage::new(phone_desc);
    phone.set_field_by_name("number", ProtoValue::String("555-0000".into()));

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name("name", ProtoValue::String("Alice".into()));
    msg.set_field_by_name("id", ProtoValue::I32(1));
    msg.set_field_by_name("phones", ProtoValue::List(vec![ProtoValue::Message(phone)]));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    // Should not error — unknown fields get skipped.
    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.column(0).as_string::<i64>().value(0), "Alice");
}

#[test]
fn absent_proto_field_becomes_all_null_column() {
    // Schema-drift path: the Arrow schema has a column `missing_col` that
    // the proto descriptor doesn't carry (simulates "the target schema has
    // the column but the producer's proto dropped it"). The encoder must emit
    // an all-null Arrow column for `missing_col` and otherwise populate normally.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("missing_col", DataType::Int64, true),
        Field::new("id", DataType::Int32, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg1 = DynamicMessage::new(desc.clone());
    msg1.set_field_by_name("name", ProtoValue::String("Alice".into()));
    msg1.set_field_by_name("id", ProtoValue::I32(1));
    let mut buf1 = Vec::new();
    msg1.encode(&mut buf1).unwrap();

    let mut msg2 = DynamicMessage::new(desc.clone());
    msg2.set_field_by_name("name", ProtoValue::String("Bob".into()));
    msg2.set_field_by_name("id", ProtoValue::I32(2));
    let mut buf2 = Vec::new();
    msg2.encode(&mut buf2).unwrap();

    let batch = enc
        .encode_batch(&[Bytes::from(buf1), Bytes::from(buf2)])
        .unwrap();
    assert_eq!(batch.num_rows(), 2);
    assert_eq!(batch.num_columns(), 3);

    assert_eq!(batch.column(0).as_string::<i64>().value(0), "Alice");
    assert_eq!(batch.column(0).as_string::<i64>().value(1), "Bob");

    // The missing column must be declared null for every row.
    let missing = batch.column(1);
    assert_eq!(missing.len(), 2);
    assert!(missing.is_null(0));
    assert!(missing.is_null(1));

    let ids = batch.column(2).as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(ids.value(0), 1);
    assert_eq!(ids.value(1), 2);
}

fn serializer_for(desc: &MessageDescriptor, schema: Schema) -> WireToArrowSerializer {
    WireToArrowSerializer::from_descriptor(desc.clone(), schema).expect("serializer build")
}

fn event_with_message_bytes(bytes: Bytes) -> Event {
    let mut e = Event::from(vector_core::event::LogEvent::default());
    e.as_mut_log().insert(event_path!("message"), bytes);
    e
}

#[test]
fn serializer_loads_descriptor_from_config() {
    // Happy path via the user-facing constructor: `desc_file` + `message_type`
    // point at a real descriptor set, `schema` is injected by the sink.
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let config = WireToArrowSerializerConfig {
        desc_file: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/protobuf/protos/test_protobuf.desc"),
        message_type: "test_protobuf.Person".to_string(),
        schema: Some(schema),
    };
    let serializer = WireToArrowSerializer::new(config).expect("build");
    assert!(matches!(
        serializer.encode_to_record_batch(&[]),
        Err(WireToArrowError::NoEvents)
    ));
}

#[test]
fn serializer_errors_on_bad_descriptor_path() {
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let config = WireToArrowSerializerConfig {
        desc_file: PathBuf::from("/nonexistent/path/to/schema.desc"),
        message_type: "some.Message".to_string(),
        schema: Some(schema),
    };
    assert!(matches!(
        WireToArrowSerializer::new(config),
        Err(WireToArrowError::DescriptorLoad { .. })
    ));
}

#[test]
fn serializer_errors_on_missing_schema() {
    let config = WireToArrowSerializerConfig {
        desc_file: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/protobuf/protos/test_protobuf.desc"),
        message_type: "test_protobuf.Person".to_string(),
        schema: None,
    };
    assert!(matches!(
        WireToArrowSerializer::new(config),
        Err(WireToArrowError::ConfigurationMissing { field: "schema" })
    ));
}

#[test]
fn serializer_empty_batch_errors() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let serializer = serializer_for(&desc, schema);
    assert!(matches!(
        serializer.encode_to_record_batch(&[]),
        Err(WireToArrowError::NoEvents)
    ));
}

#[test]
fn serializer_missing_message_field_errors() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let serializer = serializer_for(&desc, schema);

    let e1 = event_with_message_bytes(Bytes::from_static(b""));
    let e2 = Event::from(vector_core::event::LogEvent::default()); // no message

    assert!(matches!(
        serializer.encode_to_record_batch(&[e1, e2]),
        Err(WireToArrowError::MessageBytesMissing)
    ));
}

#[test]
fn serializer_wrong_type_message_errors() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let serializer = serializer_for(&desc, schema);

    // Plain strings are represented as `Value::Bytes`, so use an integer
    // to get a non-bytes variant for this negative case.
    let mut e = Event::from(vector_core::event::LogEvent::default());
    e.as_mut_log().insert(event_path!("message"), 42_i64);
    assert!(matches!(
        serializer.encode_to_record_batch(&[e]),
        Err(WireToArrowError::MessageBytesWrongType)
    ));
}

#[test]
fn serializer_end_to_end_matches_direct_encode() {
    // Build events with wire bytes on `message`, encode via the
    // serializer, and compare against calling the lower-level encoder
    // directly.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let serializer = serializer_for(&desc, schema.clone());
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut wire_bytes_list = Vec::new();
    let mut events = Vec::new();
    for i in 0..5_i32 {
        let mut msg = DynamicMessage::new(desc.clone());
        msg.set_field_by_name("name", ProtoValue::String(format!("n-{i}")));
        msg.set_field_by_name("id", ProtoValue::I32(i));
        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let bytes = Bytes::from(buf);
        wire_bytes_list.push(bytes.clone());
        events.push(event_with_message_bytes(bytes));
    }

    let via_events = serializer.encode_to_record_batch(&events).unwrap();
    let via_bytes = enc.encode_batch(&wire_bytes_list).unwrap();
    assert_eq!(via_events.num_rows(), via_bytes.num_rows());
    assert_eq!(via_events.num_columns(), via_bytes.num_columns());
    for i in 0..via_events.num_columns() {
        assert_eq!(via_events.column(i).as_ref(), via_bytes.column(i).as_ref());
    }
}

#[test]
fn repeated_scalar_unpacked_roundtrip() {
    // Unpacked: emit each element with its own tag. For proto3, this is
    // the default for non-packed repeated scalars when the writer chooses
    // not to pack (which can happen for proto2 as well).
    let desc = repeated_int32_descriptor();
    let schema = Schema::new(vec![Field::new(
        "numbers",
        DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Hand-craft wire bytes for two rows:
    //   row 0: numbers = [1, 2, 3] (unpacked: 3 tag+value pairs)
    //   row 1: numbers = [42]      (single tag+value)
    let tag = (1u8 << 3) | 0; // field 1, wire type 0 (varint)
    let mut row0 = Vec::new();
    for v in [1i32, 2, 3] {
        row0.push(tag);
        encode_varint_into(&mut row0, v as u64);
    }
    let mut row1 = Vec::new();
    row1.push(tag);
    encode_varint_into(&mut row1, 42);

    let batch = enc
        .encode_batch(&[Bytes::from(row0), Bytes::from(row1)])
        .unwrap();
    assert_eq!(batch.num_rows(), 2);
    let list = batch.column(0).as_list::<i32>();
    assert_eq!(list.value_length(0), 3);
    assert_eq!(list.value_length(1), 1);
    let values = list.values().as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(values.len(), 4);
    assert_eq!(values.value(0), 1);
    assert_eq!(values.value(1), 2);
    assert_eq!(values.value(2), 3);
    assert_eq!(values.value(3), 42);
}

#[test]
fn repeated_scalar_packed_roundtrip() {
    // Packed: one length-delimited blob with concatenated varints. proto3
    // repeated scalars default to this encoding.
    let desc = repeated_int32_descriptor();
    let schema = Schema::new(vec![Field::new(
        "numbers",
        DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Tag for field 1 with wire_type 2 (length-delimited).
    let tag = (1u8 << 3) | 2;
    let mut payload = Vec::new();
    for v in [10i32, 20, 30, 40] {
        encode_varint_into(&mut payload, v as u64);
    }
    let mut row = Vec::new();
    row.push(tag);
    encode_varint_into(&mut row, payload.len() as u64);
    row.extend_from_slice(&payload);

    let batch = enc.encode_batch(&[Bytes::from(row)]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    let list = batch.column(0).as_list::<i32>();
    assert_eq!(list.value_length(0), 4);
    let values = list.values().as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(
        (0..4).map(|i| values.value(i)).collect::<Vec<_>>(),
        vec![10, 20, 30, 40]
    );
}

#[test]
fn repeated_scalar_empty_row_produces_empty_list() {
    // A row with no tag occurrences produces an empty list, not null.
    let desc = repeated_int32_descriptor();
    let schema = Schema::new(vec![Field::new(
        "numbers",
        DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let batch = enc.encode_batch(&[Bytes::new()]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    let list = batch.column(0).as_list::<i32>();
    assert_eq!(list.value_length(0), 0);
    assert!(!list.is_null(0), "list column itself should never be null");
}

#[test]
fn map_roundtrip() {
    // Proto: test_protobuf3.Person.data = map<string, PhoneType>
    // where PhoneType is an enum (int32-encoded on the wire).
    //
    // Arrow side: Map<Struct(key: LargeUtf8, value: Int32)> with entry
    // field named "key_value" per the sink's existing convention.
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, true),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![Field::new(
        "data",
        DataType::Map(entry_field, false),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Populate a row with 2 map entries.
    let mut msg = DynamicMessage::new(desc.clone());
    let mut entries: std::collections::HashMap<prost_reflect::MapKey, ProtoValue> =
        std::collections::HashMap::new();
    entries.insert(
        prost_reflect::MapKey::String("alpha".into()),
        ProtoValue::EnumNumber(1),
    );
    entries.insert(
        prost_reflect::MapKey::String("beta".into()),
        ProtoValue::EnumNumber(2),
    );
    msg.set_field_by_name("data", ProtoValue::Map(entries));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    let map = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::MapArray>()
        .expect("column should be MapArray");
    assert_eq!(map.value_length(0), 2, "expected 2 map entries");
    let keys = map.keys().as_string::<i64>();
    let values = map.values().as_primitive::<arrow::datatypes::Int32Type>();
    // Map entry iteration order is not guaranteed — collect then compare.
    let pairs: std::collections::HashMap<String, i32> = (0..2)
        .map(|i| (keys.value(i).to_string(), values.value(i)))
        .collect();
    assert_eq!(pairs.get("alpha").copied(), Some(1));
    assert_eq!(pairs.get("beta").copied(), Some(2));
}

#[test]
fn map_entry_field_name_mismatch_rejected_at_plan_build() {
    // Arrow's Map type doesn't enforce that the inner Struct's fields are
    // named `key` and `value`, but proto MapEntry does. Declaring a Map
    // whose entry struct has a different name ("k" here) leaves no proto
    // field for the encoder to route bytes from and would force the
    // absent-padding path to hand `append_proto3_default` a kind/builder
    // pair it can't satisfy. Plan-build must reject this up front so the
    // failure surfaces at sink init rather than as a runtime panic.
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("k", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, true),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![Field::new(
        "data",
        DataType::Map(entry_field, false),
        true,
    )]);
    let err = WireToArrowEncoder::new(&desc, schema)
        .expect_err("plan-build must reject Map entry field 'k' (not in proto MapEntry)");
    assert!(
        matches!(
            err,
            WireToArrowError::MapEntryFieldNotInProto { ref name } if name == "k"
        ),
        "expected MapEntryFieldNotInProto, got {err:?}"
    );
}

/// Build a `data` field carrying one map entry, with raw bytes for the
/// MapEntry message (so we can elide the key tag, the value tag, or both —
/// proto3 default elision applies inside MapEntry messages just like every
/// other singular field). Schema: `test_protobuf3.Person.data` is field 4,
/// `map<string, PhoneType>`. Wire format: outer tag `(4 << 3) | 2 = 0x22`,
/// LEN-prefixed entry body.
fn person_with_raw_map_entry(entry_body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entry_body.len() + 2);
    out.push(0x22); // tag 4, LEN
    out.push(entry_body.len() as u8); // body length (small for tests)
    out.extend_from_slice(entry_body);
    out
}

#[test]
fn map_entry_with_empty_string_key_encodes_as_empty_string_not_null() {
    // Proto3 elides default-valued singular fields *inside* MapEntry too.
    // Arrow's Map type declares the key field non-nullable; before the fix
    // an absent key tag produced a null, which fails `StructArray::try_new`
    // at batch finish — taking the whole batch down even though the wire
    // bytes are perfectly valid proto3. Producers in prost / Python /
    // protoc-gen-cpp emit exactly this shape when given a map with key "".
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, true),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![Field::new(
        "data",
        DataType::Map(entry_field, false),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // MapEntry with only the value tag (key omitted, taking proto3 default "").
    //   0x10 = (2 << 3) | 0 = value tag, varint
    //   0x01 = enum value 1 (HOME)
    let bytes = person_with_raw_map_entry(&[0x10, 0x01]);

    let batch = enc
        .encode_batch(&[Bytes::from(bytes)])
        .expect("default-keyed map entry must not fail the batch");
    assert_eq!(batch.num_rows(), 1);
    let map = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::MapArray>()
        .expect("MapArray");
    assert_eq!(map.value_length(0), 1);
    let keys = map.keys().as_string::<i64>();
    assert_eq!(keys.value(0), "", "empty-default key must materialize as \"\"");
    assert!(!keys.is_null(0), "key column must contain no nulls");
    let values = map.values().as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(values.value(0), 1);
}

#[test]
fn map_entry_with_default_int_value_encodes_as_zero_not_null() {
    // Mirror of the key case for the value side: proto3 elides value=0
    // (default int) inside MapEntry. Even with a nullable Arrow value field,
    // the proto semantics say "absent == 0", not "absent == null" — and
    // when the Arrow value is *non*-nullable, a null here would crash the
    // batch. Set value non-nullable to exercise both behaviors at once.
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, false),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![Field::new(
        "data",
        DataType::Map(entry_field, false),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // MapEntry with only the key tag (value omitted, taking proto3 default 0).
    //   0x0a = (1 << 3) | 2 = key tag, LEN
    //   0x03 = length
    //   "foo"
    let bytes = person_with_raw_map_entry(&[0x0a, 0x03, b'f', b'o', b'o']);

    let batch = enc
        .encode_batch(&[Bytes::from(bytes)])
        .expect("default-valued map entry must not fail the batch");
    assert_eq!(batch.num_rows(), 1);
    let map = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::MapArray>()
        .unwrap();
    let keys = map.keys().as_string::<i64>();
    let values = map.values().as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(keys.value(0), "foo");
    assert_eq!(values.value(0), 0, "default-int value must materialize as 0");
    assert!(!values.is_null(0));
}

#[test]
fn map_entry_with_all_defaults_encodes_as_empty_default_pair() {
    // An entirely-empty MapEntry on the wire (`0x22 0x00`) is what prost
    // emits when you serialize `HashMap::from([("".to_string(), 0)])`.
    // Both key and value tags are elided. Must produce a valid Arrow row
    // with `("", 0)`.
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, false),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![Field::new(
        "data",
        DataType::Map(entry_field, false),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let bytes = person_with_raw_map_entry(&[]); // empty MapEntry

    let batch = enc
        .encode_batch(&[Bytes::from(bytes)])
        .expect("empty MapEntry must not fail the batch");
    assert_eq!(batch.num_rows(), 1);
    let map = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::MapArray>()
        .unwrap();
    let keys = map.keys().as_string::<i64>();
    let values = map.values().as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(keys.value(0), "");
    assert_eq!(values.value(0), 0);
}

fn encode_varint_into(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// `message Ts { int64 event_time = 1; }` — for the timestamp coercion test.
fn timestamp_descriptor() -> MessageDescriptor {
    let fd = FileDescriptorProto {
        name: Some("wire_to_arrow_test_ts.proto".into()),
        package: Some("wire_to_arrow_test".into()),
        message_type: vec![DescriptorProto {
            name: Some("Ts".into()),
            field: vec![FieldDescriptorProto {
                name: Some("event_time".into()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(ProtoType::Int64 as i32),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet { file: vec![fd] };
    let mut bytes = Vec::new();
    set.encode(&mut bytes).unwrap();
    DescriptorPool::decode(bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_to_arrow_test.Ts")
        .unwrap()
}

/// `message Choice { oneof x { int32 a = 1; string b = 2; } }`
fn oneof_descriptor() -> MessageDescriptor {
    let fd = FileDescriptorProto {
        name: Some("wire_to_arrow_test_oneof.proto".into()),
        package: Some("wire_to_arrow_test".into()),
        message_type: vec![DescriptorProto {
            name: Some("Choice".into()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("a".into()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::Int32 as i32),
                    oneof_index: Some(0),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("b".into()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::String as i32),
                    oneof_index: Some(0),
                    ..Default::default()
                },
            ],
            oneof_decl: vec![OneofDescriptorProto {
                name: Some("x".into()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet { file: vec![fd] };
    let mut bytes = Vec::new();
    set.encode(&mut bytes).unwrap();
    DescriptorPool::decode(bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_to_arrow_test.Choice")
        .unwrap()
}

#[test]
fn int64_to_timestamp_micros_coercion() {
    // proto int64 field with the Arrow column declared as Timestamp(Micro, UTC).
    let desc = timestamp_descriptor();
    let schema = Schema::new(vec![Field::new(
        "event_time",
        DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Two rows, plus one with the field absent (should produce null).
    let mut row0 = Vec::new();
    row0.push((1u8 << 3) | 0); // tag 1, varint
    encode_varint_into(&mut row0, 1_700_000_000_000_000_u64);
    let mut row1 = Vec::new();
    row1.push((1u8 << 3) | 0);
    encode_varint_into(&mut row1, 1_800_000_000_000_000_u64);

    let batch = enc
        .encode_batch(&[
            Bytes::from(row0),
            Bytes::from(row1),
            Bytes::new(), // absent -> null
        ])
        .unwrap();

    assert_eq!(batch.num_rows(), 3);
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::TimestampMicrosecondArray>()
        .expect("TimestampMicrosecondArray");
    assert_eq!(col.value(0), 1_700_000_000_000_000);
    assert_eq!(col.value(1), 1_800_000_000_000_000);
    assert!(col.is_null(2));
}

#[test]
fn oneof_variants_map_to_separate_columns() {
    // With the wire-format identity (oneof variants look like regular
    // singular fields), the encoder should populate whichever variant
    // appears in the bytes and leave the others null.
    let desc = oneof_descriptor();
    let schema = Schema::new(vec![
        Field::new("a", DataType::Int32, true),
        Field::new("b", DataType::LargeUtf8, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Row 0: only `a = 42`.
    let mut row0 = Vec::new();
    row0.push((1u8 << 3) | 0); // field 1, varint
    encode_varint_into(&mut row0, 42);

    // Row 1: only `b = "hello"`.
    let mut row1 = Vec::new();
    row1.push((2u8 << 3) | 2); // field 2, length-delimited
    encode_varint_into(&mut row1, 5);
    row1.extend_from_slice(b"hello");

    let batch = enc
        .encode_batch(&[Bytes::from(row0), Bytes::from(row1)])
        .unwrap();
    assert_eq!(batch.num_rows(), 2);

    let a = batch.column(0).as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(a.value(0), 42);
    assert!(a.is_null(1));

    let b = batch.column(1).as_string::<i64>();
    assert!(batch.column(1).is_null(0));
    assert_eq!(b.value(1), "hello");
}

/// `message Tree { Tree next = 1; int32 leaf = 2; }` — a proto that's
/// self-referential by construction, used to drive deep plan/scan recursion.
fn self_referential_descriptor() -> MessageDescriptor {
    let fd = FileDescriptorProto {
        name: Some("wire_to_arrow_test_tree.proto".into()),
        package: Some("wire_to_arrow_test".into()),
        message_type: vec![DescriptorProto {
            name: Some("Tree".into()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("next".into()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::Message as i32),
                    type_name: Some(".wire_to_arrow_test.Tree".into()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("leaf".into()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::Int32 as i32),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet { file: vec![fd] };
    let mut bytes = Vec::new();
    set.encode(&mut bytes).unwrap();
    DescriptorPool::decode(bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_to_arrow_test.Tree")
        .unwrap()
}

/// Build an Arrow Struct nested `levels` deep along a single `next` field,
/// with an `Int32` `leaf` at every level. Used to drive `MessagePlan::build`
/// recursion to a known depth.
fn nested_tree_struct(levels: usize) -> DataType {
    if levels == 0 {
        // Innermost level: just the leaf scalar, no `next`.
        return DataType::Struct(ArrowFields::from(vec![Field::new(
            "leaf",
            DataType::Int32,
            true,
        )]));
    }
    DataType::Struct(ArrowFields::from(vec![
        Field::new("next", nested_tree_struct(levels - 1), true),
        Field::new("leaf", DataType::Int32, true),
    ]))
}

#[test]
fn plan_build_rejects_schema_deeper_than_cap() {
    use super::plan::MAX_NESTING_DEPTH;

    let desc = self_referential_descriptor();
    // One level over the cap. The wrapper Schema counts as depth 0, so we
    // need MAX_NESTING_DEPTH levels of nested struct to step over the limit.
    let deep_struct = nested_tree_struct(MAX_NESTING_DEPTH);
    let schema = Schema::new(vec![Field::new("next", deep_struct, true)]);
    let err = WireToArrowEncoder::new(&desc, schema).expect_err("should reject");
    assert!(
        matches!(err, WireToArrowError::SchemaTooDeep { limit } if limit == MAX_NESTING_DEPTH),
        "expected SchemaTooDeep, got {err:?}"
    );
}

#[test]
fn plan_build_accepts_moderately_deep_schema() {
    // A reasonably deep but legal schema must build without error. Pick a
    // depth far below the cap so any reasonable real-world nesting is fine.
    let desc = self_referential_descriptor();
    let deep_struct = nested_tree_struct(8);
    let schema = Schema::new(vec![Field::new("next", deep_struct, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).expect("8-deep schema should build");
    // And it should be able to encode an empty payload (every level absent).
    let batch = enc.encode_batch(&[Bytes::new()]).unwrap();
    assert_eq!(batch.num_rows(), 1);
}

#[test]
fn plan_build_rejects_non_nullable_singular_scalar() {
    // proto3 omits default-valued singular scalars on the wire, so a
    // column declared non-nullable would fail RecordBatch::try_new with
    // a generic Arrow error deep in encode_batch — dropping the whole
    // batch. The plan builder should reject this at init.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, /* nullable */ false),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let err = WireToArrowEncoder::new(&desc, schema).expect_err("should reject");
    assert!(
        matches!(
            &err,
            WireToArrowError::NonNullableNotGuaranteed { name, .. } if name == "id"
        ),
        "expected NonNullableNotGuaranteed for 'id', got {err:?}"
    );
}

#[test]
fn plan_build_allows_non_nullable_outer_list_and_map() {
    // Repeated and map outer columns are always-present (the encoder
    // emits an empty list / empty map for an absent occurrence), so a
    // non-nullable declaration on the outer column is safe and must
    // build without error.
    let desc = rich_descriptor();
    let phone_struct = DataType::Struct(ArrowFields::from(vec![Field::new(
        "number",
        DataType::LargeUtf8,
        true,
    )]));
    let phones_field = Field::new("item", phone_struct, true);
    let entry_fields = ArrowFields::from(vec![
        // Arrow Map keys are mandated non-nullable by the Map type
        // contract; the carve-out for Map entry sub-plans must let this
        // through.
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, true),
    ]);
    let entry_field = Arc::new(Field::new(
        "key_value",
        DataType::Struct(entry_fields),
        false,
    ));
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        // Outer list non-nullable: OK, the encoder writes empty-list, not null.
        Field::new(
            "phones",
            DataType::List(Arc::new(phones_field)),
            /* nullable */ false,
        ),
        // Outer map non-nullable: OK, same reason.
        Field::new("data", DataType::Map(entry_field, false), /* nullable */ false),
    ]);
    WireToArrowEncoder::new(&desc, schema).expect("should build");
}

#[test]
fn plan_build_rejects_non_nullable_absent_column() {
    // Schema-drift case: Arrow schema has a column the proto descriptor
    // doesn't carry. The encoder fills it with all nulls; a non-nullable
    // declaration is a hard mismatch that must be rejected at init.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("dropped_from_proto", DataType::Int64, /* nullable */ false),
    ]);
    let err = WireToArrowEncoder::new(&desc, schema).expect_err("should reject");
    assert!(
        matches!(
            &err,
            WireToArrowError::NonNullableNotGuaranteed { name, .. }
                if name == "dropped_from_proto"
        ),
        "expected NonNullableNotGuaranteed for absent column, got {err:?}"
    );
}

#[test]
fn plan_build_rejects_unsupported_arrow_leaf_in_scalar_slot() {
    // proto says `id: int32`, Arrow says `id: Date32`. Date32 isn't in
    // `TypedBuilder::supports`, so plan-build must reject up front
    // rather than letting the first batch panic inside `TypedBuilder::new`.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Date32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let err = WireToArrowEncoder::new(&desc, schema).expect_err("should reject Date32");
    assert!(
        matches!(
            &err,
            WireToArrowError::UnsupportedArrowLeafType { name, .. } if name == "id"
        ),
        "expected UnsupportedArrowLeafType for 'id', got {err:?}"
    );
}

#[test]
fn plan_build_rejects_unsupported_arrow_leaf_in_absent_slot() {
    // `created_at` doesn't exist in test_protobuf.Person, so it becomes
    // PlanSlot::Absent. The Absent path builds via `build_absent_node`,
    // which also calls `TypedBuilder::new` for leaves. Plan-build must
    // validate the Arrow leaf type on absent slots too.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("created_at", DataType::Date32, true),
    ]);
    let err = WireToArrowEncoder::new(&desc, schema)
        .expect_err("should reject Date32 on absent slot");
    assert!(
        matches!(
            &err,
            WireToArrowError::UnsupportedArrowLeafType { name, .. } if name == "created_at"
        ),
        "expected UnsupportedArrowLeafType for 'created_at', got {err:?}"
    );
}

#[test]
fn multiple_rows_preserve_order() {
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("id", DataType::Int32, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut messages = Vec::new();
    for i in 0..5 {
        let mut msg = DynamicMessage::new(desc.clone());
        msg.set_field_by_name("id", ProtoValue::I32(i * 10));
        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        messages.push(Bytes::from(buf));
    }

    let batch = enc.encode_batch(&messages).unwrap();
    let ids = batch.column(0).as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(ids.len(), 5);
    for i in 0..5 {
        assert_eq!(ids.value(i), (i as i32) * 10);
    }
}

#[test]
fn wire_descriptor_tags_drive_decode_regardless_of_arrow_schema_source() {
    // The two mappings the encoder relies on:
    //   (a) Arrow schema ↔ proto descriptor — by name, at plan-build time.
    //   (b) Proto descriptor ↔ wire bytes — by tag number, at scan time.
    // They are independent. An Arrow schema derived from any other
    // descriptor (e.g. one synthesized with position-based tags) must
    // not leak its tag numbers into the scan. Tags on the wire come from
    // whatever descriptor the *wire* side was encoded with, and that's the
    // only descriptor the serializer is told about.
    //
    // Here: wire descriptor uses tags 1001/1002/1003. If the scanner ever
    // fell back to a position-based or otherwise-synthesized tag space
    // (1/2/3), every wire tag would miss and the batch columns would be
    // all-null. Full population proves decode uses the wire descriptor's
    // tag numbers exclusively.
    let wire_fd = FileDescriptorProto {
        name: Some("wire_tag_divergence_test.proto".into()),
        package: Some("wire_tag_divergence_test".into()),
        message_type: vec![DescriptorProto {
            name: Some("Row".into()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("name".into()),
                    number: Some(1001),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::String as i32),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("id".into()),
                    number: Some(1002),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::Int32 as i32),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("email".into()),
                    number: Some(1003),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::String as i32),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet {
        file: vec![wire_fd],
    };
    let mut set_bytes = Vec::new();
    set.encode(&mut set_bytes).unwrap();
    let wire_desc = DescriptorPool::decode(set_bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_tag_divergence_test.Row")
        .unwrap();

    let arrow_schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
        Field::new("email", DataType::LargeUtf8, true),
    ]);
    let serializer = WireToArrowSerializer::from_descriptor(wire_desc.clone(), arrow_schema)
        .expect("serializer build");

    let mut msg = DynamicMessage::new(wire_desc);
    msg.set_field_by_name("name", ProtoValue::String("alice".into()));
    msg.set_field_by_name("id", ProtoValue::I32(42));
    msg.set_field_by_name("email", ProtoValue::String("alice@example.com".into()));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = serializer
        .encode_to_record_batch(&[event_with_message_bytes(Bytes::from(buf))])
        .expect("encode");
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 3);
    assert_eq!(batch.column(0).as_string::<i64>().value(0), "alice");
    assert_eq!(
        batch
            .column(1)
            .as_primitive::<arrow::datatypes::Int32Type>()
            .value(0),
        42
    );
    assert_eq!(
        batch.column(2).as_string::<i64>().value(0),
        "alice@example.com"
    );
}

// -------------------------------------------------------------------------
// Per-row isolation: a single malformed message in a batch must not poison
// the whole batch. The encoder pre-validates each message and drops bad
// rows from the output `RecordBatch`, counting them via the
// `wire_to_arrow_rows_dropped` metric.
// -------------------------------------------------------------------------

#[test]
fn encode_batch_drops_malformed_row_in_mixed_batch() {
    // A single malformed message in the middle of an otherwise-valid batch
    // must not fail the batch. The bad row is dropped; the valid rows still
    // appear in the output `RecordBatch` in original order.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut a = DynamicMessage::new(desc.clone());
    a.set_field_by_name("name", ProtoValue::String("alice".into()));
    a.set_field_by_name("id", ProtoValue::I32(1));
    let mut buf_a = Vec::new();
    a.encode(&mut buf_a).unwrap();

    let mut b = DynamicMessage::new(desc.clone());
    b.set_field_by_name("name", ProtoValue::String("bob".into()));
    b.set_field_by_name("id", ProtoValue::I32(2));
    let mut buf_b = Vec::new();
    b.encode(&mut buf_b).unwrap();

    // Tag = (1 << 3) | 2 = 0x0a (`name`, LEN); declared length 5, only 2
    // payload bytes follow → BufferTooShort inside `try_parse_field`,
    // which the encoder maps to `UnexpectedEof`.
    let bad = vec![0x0a, 0x05, 0x01, 0x02];

    let batch = enc
        .encode_batch(&[Bytes::from(buf_a), Bytes::from(bad), Bytes::from(buf_b)])
        .expect("malformed row must not fail the batch");
    assert_eq!(batch.num_rows(), 2);
    let names = batch.column(0).as_string::<i64>();
    assert_eq!(names.value(0), "alice");
    assert_eq!(names.value(1), "bob");
    let ids = batch.column(1).as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(ids.value(0), 1);
    assert_eq!(ids.value(1), 2);
}

#[test]
fn encode_batch_drops_invalid_utf8_row() {
    // String field with non-UTF-8 payload — the pre-validate pass catches
    // this via the `from_utf8` check in `validate_scalar_from_wire`.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("name", DataType::LargeUtf8, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Tag = (1 << 3) | 2 = 0x0a (`name`, LEN); length = 1; payload = 0xff
    // (lone continuation byte, not a valid UTF-8 sequence).
    let bad = vec![0x0a, 0x01, 0xff];

    let mut ok = DynamicMessage::new(desc.clone());
    ok.set_field_by_name("name", ProtoValue::String("ok".into()));
    let mut ok_buf = Vec::new();
    ok.encode(&mut ok_buf).unwrap();

    let batch = enc
        .encode_batch(&[Bytes::from(bad), Bytes::from(ok_buf)])
        .expect("invalid-UTF-8 row must not fail the batch");
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.column(0).as_string::<i64>().value(0), "ok");
}

#[test]
fn encode_batch_all_malformed_returns_empty_batch() {
    // Every row malformed → empty `RecordBatch` (schema preserved, zero
    // rows). Surfacing the situation is the metric's job; the serializer
    // doesn't conflate "every row was bad" with "configuration broken".
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![Field::new("name", DataType::LargeUtf8, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let bad1 = vec![0x0a, 0x05, 0x00, 0x01]; // truncated LEN value
    let bad2 = vec![0x0a, 0x01, 0xff]; // invalid UTF-8

    let batch = enc
        .encode_batch(&[Bytes::from(bad1), Bytes::from(bad2)])
        .expect("all-malformed batch must still return an empty RecordBatch");
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.num_columns(), 1);
}

#[test]
fn encode_batch_drops_packed_scalar_eof_row() {
    // Packed repeated int32 with a truncated varint in the inner blob —
    // the one site (`append_repeated_scalar`'s packed loop) where the
    // real scan could otherwise leave half the elements committed before
    // erroring. Pre-validate sees the EOF and drops the row before any
    // append.
    let desc = repeated_int32_descriptor();
    let schema = Schema::new(vec![Field::new(
        "numbers",
        DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Tag = (1 << 3) | 2 = 0x0a (`numbers`, LEN — packed form); length 2;
    // inner = [0x80, 0x80] — a varint with continuation bits and no
    // terminator → EOF inside the packed walk.
    let bad = vec![0x0a, 0x02, 0x80, 0x80];
    // Packed encoding of `numbers = [7]`: one varint (7) inside a LEN blob.
    let good = vec![0x0a, 0x01, 0x07];

    let batch = enc
        .encode_batch(&[Bytes::from(bad), Bytes::from(good)])
        .expect("packed EOF row must not fail the batch");
    assert_eq!(batch.num_rows(), 1);
    let list = batch.column(0).as_list::<i32>();
    assert_eq!(list.value_length(0), 1);
}

#[test]
fn encode_batch_drops_row_with_duplicate_singular_scalar_tag() {
    // Proto3 parsers must accept duplicate singular tags (last-wins for
    // scalars), but the encoder appends to Arrow column builders on every
    // occurrence, so a second tag would diverge column lengths and fail
    // `RecordBatch::try_new`. `validate_message` detects the duplicate and
    // drops the row before any builder is touched.
    let desc = scalar_descriptor();
    let schema = Schema::new(vec![
        Field::new("name", DataType::LargeUtf8, true),
        Field::new("id", DataType::Int32, true),
    ]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut a = DynamicMessage::new(desc.clone());
    a.set_field_by_name("name", ProtoValue::String("alice".into()));
    a.set_field_by_name("id", ProtoValue::I32(1));
    let mut buf_a = Vec::new();
    a.encode(&mut buf_a).unwrap();

    let mut b = DynamicMessage::new(desc.clone());
    b.set_field_by_name("name", ProtoValue::String("bob".into()));
    b.set_field_by_name("id", ProtoValue::I32(2));
    let mut buf_b = Vec::new();
    b.encode(&mut buf_b).unwrap();

    // Hand-rolled bytes: two occurrences of singular tag 1 (`name`, LEN).
    //   0x0a 0x03 "bob"     — first occurrence, value "bob"
    //   0x0a 0x05 "alice"   — second occurrence, value "alice"
    // Spec says last-wins ("alice"); the encoder cannot honor that without
    // builder retraction, so the row must be dropped via validate.
    let dup = vec![
        0x0a, 0x03, b'b', b'o', b'b', 0x0a, 0x05, b'a', b'l', b'i', b'c', b'e',
    ];

    let batch = enc
        .encode_batch(&[Bytes::from(buf_a), Bytes::from(dup), Bytes::from(buf_b)])
        .expect("duplicate-tag row must not fail the batch");
    assert_eq!(batch.num_rows(), 2);
    let names = batch.column(0).as_string::<i64>();
    assert_eq!(names.value(0), "alice");
    assert_eq!(names.value(1), "bob");
    let ids = batch.column(1).as_primitive::<arrow::datatypes::Int32Type>();
    assert_eq!(ids.value(0), 1);
    assert_eq!(ids.value(1), 2);
}

#[test]
fn append_repeated_scalar_unpacked_offset_overflow_is_clean_error() {
    // The Arrow `ListArray` uses i32 offsets, so `current_offset` can grow
    // at most to `i32::MAX` across a batch. Without bounds checking the
    // `+= 1` wraps in release mode and `OffsetBuffer::new` later asserts at
    // batch finish, panicking the process. The encoder uses `checked_add`
    // here so an overflow surfaces as a structured `OffsetOverflow` error.
    // This converts a process-panic surface (adversarial input → crash) into
    // a clean batch-level failure.
    use super::append::append_repeated_scalar;
    use super::builders::TypedBuilder;
    let mut values = TypedBuilder::new(&DataType::Int32, 1);
    let mut current_offset: i32 = i32::MAX;
    let wv = WireValue::Varint(42);
    let err = append_repeated_scalar(ScalarKind::Int32, &wv, &mut values, &mut current_offset)
        .expect_err("unpacked repeated-scalar increment at i32::MAX must error");
    assert!(
        matches!(err, WireToArrowError::OffsetOverflow { .. }),
        "expected OffsetOverflow, got {err:?}",
    );
}

#[test]
fn append_repeated_scalar_packed_offset_overflow_is_clean_error() {
    // Same property for the packed-blob inner loop. A two-element packed
    // varint starting from `current_offset == i32::MAX - 1` overflows on
    // the second element; the loop's `checked_add` must surface that as
    // `OffsetOverflow` rather than wrapping silently.
    use super::append::append_repeated_scalar;
    use super::builders::TypedBuilder;
    let mut values = TypedBuilder::new(&DataType::Int32, 4);
    // Two varint elements (7, 8) packed inline. We synthesize at the
    // WireValue layer, so the outer tag + length prefix have already been
    // stripped — only the inner packed payload appears here.
    let blob: Vec<u8> = vec![0x07, 0x08];
    let wv = WireValue::Len(&blob);
    let mut current_offset: i32 = i32::MAX - 1;
    let err = append_repeated_scalar(ScalarKind::Int32, &wv, &mut values, &mut current_offset)
        .expect_err("packed-scalar increment crossing i32::MAX must error");
    assert!(
        matches!(err, WireToArrowError::OffsetOverflow { .. }),
        "expected OffsetOverflow, got {err:?}",
    );
}

#[test]
fn map_preserves_user_entry_field_name_and_metadata() {
    // Arrow's Map spec doesn't pin a specific entry name — "entries" is
    // canonical in Arrow itself; "key_value" is the Spark/Delta convention.
    // Before the fix the encoder hardcoded
    // "key_value" at finish time, so any caller whose schema declared a
    // different entry name (or attached metadata) got `RecordBatch::try_new`
    // rejection on every batch. The fix preserves the user-supplied entry
    // Field unchanged.
    let desc = rich_descriptor();
    let entry_fields = ArrowFields::from(vec![
        Field::new("key", DataType::LargeUtf8, false),
        Field::new("value", DataType::Int32, true),
    ]);
    let user_entry = Arc::new(
        Field::new("entries", DataType::Struct(entry_fields), false).with_metadata(
            [("source".to_string(), "unit_test".to_string())]
                .into_iter()
                .collect(),
        ),
    );
    let outer_field = Field::new("data", DataType::Map(Arc::clone(&user_entry), false), true);
    let schema = Schema::new(vec![outer_field.clone()]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg = DynamicMessage::new(desc.clone());
    let mut entries: std::collections::HashMap<prost_reflect::MapKey, ProtoValue> =
        std::collections::HashMap::new();
    entries.insert(
        prost_reflect::MapKey::String("k1".into()),
        ProtoValue::EnumNumber(1),
    );
    msg.set_field_by_name("data", ProtoValue::Map(entries));
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc
        .encode_batch(&[Bytes::from(buf)])
        .expect("batch must finish with the user-supplied entry name preserved");
    assert_eq!(batch.num_rows(), 1);
    // The schema the batch reports must match what we declared, including
    // the non-default entry name and the metadata we attached.
    let col_field = batch.schema().field(0).clone();
    let actual_entry = match col_field.data_type() {
        DataType::Map(f, _) => Arc::clone(f),
        other => panic!("expected Map, got {other:?}"),
    };
    assert_eq!(actual_entry.name(), "entries");
    assert_eq!(
        actual_entry.metadata().get("source").map(String::as_str),
        Some("unit_test"),
        "user-attached entry metadata must round-trip",
    );
}

// -------------------------------------------------------------------------
// Enum -> STRING column (PlanSlot::EnumString): render the proto enum varint
// as its value name, matching the arrow_stream / `proto_to_value` path.
// -------------------------------------------------------------------------

/// Build `message Msg { Outcome outcome = 1; }` with
/// `enum Outcome { UNSPECIFIED = 0; SUCCESS = 1; FAILURE = 2; }`.
fn singular_enum_descriptor() -> MessageDescriptor {
    use prost_reflect::prost_types::{EnumDescriptorProto, EnumValueDescriptorProto};
    let fd = FileDescriptorProto {
        name: Some("wire_to_arrow_enum_test.proto".into()),
        package: Some("wire_to_arrow_enum_test".into()),
        syntax: Some("proto3".into()),
        enum_type: vec![EnumDescriptorProto {
            name: Some("Outcome".into()),
            value: vec![
                EnumValueDescriptorProto {
                    name: Some("UNSPECIFIED".into()),
                    number: Some(0),
                    ..Default::default()
                },
                EnumValueDescriptorProto {
                    name: Some("SUCCESS".into()),
                    number: Some(1),
                    ..Default::default()
                },
                EnumValueDescriptorProto {
                    name: Some("FAILURE".into()),
                    number: Some(2),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        message_type: vec![DescriptorProto {
            name: Some("Msg".into()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("outcome".into()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(ProtoType::Enum as i32),
                    type_name: Some(".wire_to_arrow_enum_test.Outcome".into()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("outcomes".into()),
                    number: Some(2),
                    label: Some(Label::Repeated as i32),
                    r#type: Some(ProtoType::Enum as i32),
                    type_name: Some(".wire_to_arrow_enum_test.Outcome".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let set = FileDescriptorSet { file: vec![fd] };
    let mut bytes = Vec::new();
    set.encode(&mut bytes).unwrap();
    DescriptorPool::decode(bytes.as_slice())
        .unwrap()
        .get_message_by_name("wire_to_arrow_enum_test.Msg")
        .unwrap()
}

#[test]
fn enum_to_string_renders_value_name() {
    let desc = singular_enum_descriptor();
    let schema = Schema::new(vec![Field::new("outcome", DataType::LargeUtf8, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name("outcome", ProtoValue::EnumNumber(1)); // SUCCESS
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    assert_eq!(batch.column(0).as_string::<i64>().value(0), "SUCCESS");
}

#[test]
fn enum_to_string_absent_is_null() {
    // proto3 elides an enum at its zero value; `proto_to_value` only walks
    // present fields, so an absent enum -> null (NOT the name of value 0).
    let desc = singular_enum_descriptor();
    let schema = Schema::new(vec![Field::new("outcome", DataType::LargeUtf8, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    // Empty message: `outcome` absent.
    let batch = enc.encode_batch(&[Bytes::new()]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert!(batch.column(0).is_null(0), "absent enum must be null");
}

#[test]
fn enum_to_string_unknown_value_renders_placeholder() {
    // An out-of-range enum number (e.g. a value added after this binary was
    // built) has no descriptor entry. The encoder renders an
    // `UNKNOWN_ENUM_VALUE_<enum>_<n>` placeholder rather than dropping the row.
    let desc = singular_enum_descriptor();
    let schema = Schema::new(vec![Field::new("outcome", DataType::LargeUtf8, true)]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut good = DynamicMessage::new(desc.clone());
    good.set_field_by_name("outcome", ProtoValue::EnumNumber(2)); // FAILURE
    let mut good_buf = Vec::new();
    good.encode(&mut good_buf).unwrap();

    let mut unknown = DynamicMessage::new(desc.clone());
    unknown.set_field_by_name("outcome", ProtoValue::EnumNumber(99)); // undefined
    let mut unknown_buf = Vec::new();
    unknown.encode(&mut unknown_buf).unwrap();

    let batch = enc
        .encode_batch(&[Bytes::from(good_buf), Bytes::from(unknown_buf)])
        .expect("unknown-enum row must not fail the batch");
    assert_eq!(batch.num_rows(), 2, "the unknown-enum row must be kept");
    let col = batch.column(0).as_string::<i64>();
    assert_eq!(col.value(0), "FAILURE");
    assert_eq!(col.value(1), "UNKNOWN_ENUM_VALUE_Outcome_99");
}

#[test]
fn repeated_enum_to_string_renders_value_names() {
    // `repeated Outcome` -> List<LargeUtf8>: each element rendered by name.
    let desc = singular_enum_descriptor();
    let outcomes = Field::new("item", DataType::LargeUtf8, true);
    let schema = Schema::new(vec![Field::new(
        "outcomes",
        DataType::List(Arc::new(outcomes)),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name(
        "outcomes",
        ProtoValue::List(vec![
            ProtoValue::EnumNumber(1), // SUCCESS
            ProtoValue::EnumNumber(2), // FAILURE
            ProtoValue::EnumNumber(0), // UNSPECIFIED
        ]),
    );
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc.encode_batch(&[Bytes::from(buf)]).unwrap();
    let list = batch.column(0).as_list::<i32>();
    let vals = list.value(0);
    let strs = vals.as_string::<i64>();
    assert_eq!(strs.len(), 3);
    assert_eq!(strs.value(0), "SUCCESS");
    assert_eq!(strs.value(1), "FAILURE");
    assert_eq!(strs.value(2), "UNSPECIFIED");
}

#[test]
fn repeated_enum_to_string_empty_when_absent() {
    // An absent repeated field is an empty list (never null), like every
    // other repeated slot.
    let desc = singular_enum_descriptor();
    let outcomes = Field::new("item", DataType::LargeUtf8, true);
    let schema = Schema::new(vec![Field::new(
        "outcomes",
        DataType::List(Arc::new(outcomes)),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let batch = enc.encode_batch(&[Bytes::new()]).unwrap();
    assert_eq!(batch.num_rows(), 1);
    let list = batch.column(0).as_list::<i32>();
    assert!(!list.is_null(0), "absent repeated enum must be empty list, not null");
    assert_eq!(list.value(0).len(), 0);
}

#[test]
fn repeated_enum_to_string_unknown_value_renders_placeholder() {
    // An out-of-range element renders its placeholder in place (parity with the
    // singular case); the row is kept and known elements are unaffected.
    let desc = singular_enum_descriptor();
    let outcomes = Field::new("item", DataType::LargeUtf8, true);
    let schema = Schema::new(vec![Field::new(
        "outcomes",
        DataType::List(Arc::new(outcomes)),
        true,
    )]);
    let enc = WireToArrowEncoder::new(&desc, schema).unwrap();

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field_by_name(
        "outcomes",
        ProtoValue::List(vec![ProtoValue::EnumNumber(1), ProtoValue::EnumNumber(99)]),
    );
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let batch = enc
        .encode_batch(&[Bytes::from(buf)])
        .expect("unknown-enum element must not fail the batch");
    assert_eq!(batch.num_rows(), 1, "the row with an unknown element must be kept");
    let list = batch.column(0).as_list::<i32>();
    let strs = list.value(0);
    let strs = strs.as_string::<i64>();
    assert_eq!(strs.len(), 2);
    assert_eq!(strs.value(0), "SUCCESS");
    assert_eq!(strs.value(1), "UNKNOWN_ENUM_VALUE_Outcome_99");
}

// -------------------------------------------------------------------------
// Fuzz: random wire bytes through `encode_batch` must not panic. Any
// `Result` outcome is acceptable — we only care that bad input is reported
// as a normal error and that the scan-time recursion (which the depth cap
// also bounds) doesn't overflow the stack.
// -------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        // Per-case timeout in ms; bounds CI cost if a future change makes
        // the scan path much slower for some inputs.
        timeout: 2_000,
        ..ProptestConfig::default()
    })]

    /// Encoder must never panic on adversarial wire bytes against a scalar
    /// schema. Most random byte sequences will hit `UnexpectedEof`,
    /// `InvalidWireType`, or `WireTypeMismatch`; a few will parse but
    /// produce nonsense values. All paths are fine as long as no panic.
    #[test]
    fn encode_batch_does_not_panic_on_random_bytes_scalar(
        bytes in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let desc = scalar_descriptor();
        let schema = Schema::new(vec![
            Field::new("name", DataType::LargeUtf8, true),
            Field::new("id", DataType::Int32, true),
            Field::new("email", DataType::LargeUtf8, true),
        ]);
        let enc = WireToArrowEncoder::new(&desc, schema).unwrap();
        // Any Result is acceptable — the assertion is "no panic".
        let _ = enc.encode_batch(&[Bytes::from(bytes)]);
    }

    /// Same property against a deeply-nestable self-referential descriptor.
    /// This is the wire-side stack-overflow surface Flavio called out:
    /// attacker-controlled bytes try to drive `scan_message` recursion
    /// down to the plan's maximum depth. With the depth cap in place,
    /// scanning bounded by the plan stays within the safe limit.
    #[test]
    fn encode_batch_does_not_panic_on_random_bytes_nested(
        bytes in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let desc = self_referential_descriptor();
        // Modest nesting in the Arrow schema — well under the cap, but
        // enough that adversarial bytes have a real `next` field to chase.
        let deep_struct = nested_tree_struct(8);
        let schema = Schema::new(vec![Field::new("next", deep_struct, true)]);
        let enc = WireToArrowEncoder::new(&desc, schema).unwrap();
        let _ = enc.encode_batch(&[Bytes::from(bytes)]);
    }
}
