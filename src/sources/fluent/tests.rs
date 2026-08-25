use bytes::BytesMut;
use chrono::{DateTime, Utc};
use rmp_serde::Serializer;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::{Duration, error::Elapsed, timeout},
};
use tokio_util::codec::Decoder;
use vector_lib::{assert_event_data_eq, lookup::OwnedTargetPath, schema::Definition};
use vrl::event_path;
use vrl::value::{ObjectMap, Value, kind::Collection};

use super::{message::FluentMessageOptions, *};
use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    event::EventStatus,
    test_util::{self, addr::next_addr, trace_init, wait_for_tcp},
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<FluentConfig>();
}

// useful references for msgpack:
// Spec: https://github.com/msgpack/msgpack/blob/master/spec.md
// Encode to array of bytes: https://kawanet.github.io/msgpack-lite/
// Decode base64: https://toolslick.com/conversion/data/messagepack-to-json

fn mock_event(name: &str, timestamp: &str) -> Event {
    Event::Log(LogEvent::from(ObjectMap::from([
        ("message".into(), Value::from(name)),
        (
            log_schema().source_type_key().unwrap().to_string().into(),
            Value::from(FluentConfig::NAME),
        ),
        ("tag".into(), Value::from("tag.name")),
        (
            "timestamp".into(),
            Value::Timestamp(DateTime::parse_from_rfc3339(timestamp).unwrap().into()),
        ),
    ])))
}

#[test]
fn decode_message_mode() {
    //[
    //  "tag.name",
    //  1441588984,
    //  {"message": "bar"},
    //]
    let message: Vec<u8> = vec![
        147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 206, 85, 236, 230, 248, 129, 167, 109, 101,
        115, 115, 97, 103, 101, 163, 98, 97, 114,
    ];

    let expected = mock_event("bar", "2015-09-07T01:23:04Z");
    let got = decode_all(message.clone()).unwrap();
    assert_event_data_eq!(got.0[0], expected);
    assert_eq!(got.1, message.len());
}

#[test]
fn decode_message_mode_with_options() {
    //[
    //  "tag.name",
    //   1441588984,
    //   { "message": "bar" },
    //   { "size": 1 }
    //]
    let message: Vec<u8> = vec![
        148, 168, 116, 97, 103, 46, 110, 97, 109, 101, 206, 85, 236, 230, 248, 129, 167, 109, 101,
        115, 115, 97, 103, 101, 163, 98, 97, 114, 129, 164, 115, 105, 122, 101, 1,
    ];

    let expected = mock_event("bar", "2015-09-07T01:23:04Z");
    let got = decode_all(message.clone()).unwrap();
    assert_eq!(got.1, message.len());
    assert_event_data_eq!(got.0[0], expected);
}

#[test]
fn decode_forward_mode() {
    //[
    //    "tag.name",
    //    [
    //        [1441588984, {"message": "foo"}],
    //        [1441588985, {"message": "bar"}],
    //        [1441588986, {"message": "baz"}]
    //    ]
    //]
    let message: Vec<u8> = vec![
        146, 168, 116, 97, 103, 46, 110, 97, 109, 101, 147, 146, 206, 85, 236, 230, 248, 129, 167,
        109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230, 249, 129,
        167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236, 230, 250, 129,
        167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122,
    ];

    let expected = [
        mock_event("foo", "2015-09-07T01:23:04Z"),
        mock_event("bar", "2015-09-07T01:23:05Z"),
        mock_event("baz", "2015-09-07T01:23:06Z"),
    ];
    let got = decode_all(message.clone()).unwrap();

    assert_eq!(got.1, message.len());
    assert_event_data_eq!(got.0[0], expected[0]);
    assert_event_data_eq!(got.0[1], expected[1]);
    assert_event_data_eq!(got.0[2], expected[2]);
}

#[test]
fn decode_forward_mode_with_options() {
    //[
    //    "tag.name",
    //    [
    //        [1441588984, {"message": "foo"}],
    //        [1441588985, {"message": "bar"}],
    //        [1441588986, {"message": "baz"}]
    //    ],
    //    {"size": 3}
    //]
    let message: Vec<u8> = vec![
        147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 147, 146, 206, 85, 236, 230, 248, 129, 167,
        109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230, 249, 129,
        167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236, 230, 250, 129,
        167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122, 129, 164, 115, 105, 122, 101, 3,
    ];

    let expected = [
        mock_event("foo", "2015-09-07T01:23:04Z"),
        mock_event("bar", "2015-09-07T01:23:05Z"),
        mock_event("baz", "2015-09-07T01:23:06Z"),
    ];

    let got = decode_all(message.clone()).unwrap();

    assert_eq!(got.1, message.len());

    assert_event_data_eq!(got.0[0], expected[0]);
    assert_event_data_eq!(got.0[1], expected[1]);
    assert_event_data_eq!(got.0[2], expected[2]);
}

#[test]
fn decode_packed_forward_mode() {
    //[
    //    "tag.name",
    //    <packed messages>
    //]
    //
    //With packed messages as bin:
    // [1441588984, {"message": "foo"}]
    // [1441588985, {"message": "bar"}]
    // [1441588986, {"message": "baz"}]
    let message: Vec<u8> = vec![
        147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 196, 57, 146, 206, 85, 236, 230, 248, 129,
        167, 109, 101, 115, 115, 97, 103, 101, 163, 102, 111, 111, 146, 206, 85, 236, 230, 249,
        129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 114, 146, 206, 85, 236, 230, 250,
        129, 167, 109, 101, 115, 115, 97, 103, 101, 163, 98, 97, 122, 129, 167, 109, 101, 115, 115,
        97, 103, 101, 163, 102, 111, 111,
    ];

    let expected = [
        mock_event("foo", "2015-09-07T01:23:04Z"),
        mock_event("bar", "2015-09-07T01:23:05Z"),
        mock_event("baz", "2015-09-07T01:23:06Z"),
    ];

    let got = decode_all(message.clone()).unwrap();

    assert_eq!(got.1, message.len());
    assert_event_data_eq!(got.0[0], expected[0]);
    assert_event_data_eq!(got.0[1], expected[1]);
    assert_event_data_eq!(got.0[2], expected[2]);
}

//  TODO
#[test]
fn decode_compressed_packed_forward_mode() {
    //[
    //    "tag.name",
    //    <packed messages>,
    //    {"compressed": "gzip"}
    //]
    //
    //With gzip'd packed messages as bin:
    // [1441588984, {"message": "foo"}]
    // [1441588985, {"message": "bar"}]
    // [1441588986, {"message": "baz"}]
    let message: Vec<u8> = vec![
        147, 168, 116, 97, 103, 46, 110, 97, 109, 101, 196, 55, 31, 139, 8, 0, 245, 10, 168, 96, 0,
        3, 155, 116, 46, 244, 205, 179, 31, 141, 203, 115, 83, 139, 139, 19, 211, 83, 23, 167, 229,
        231, 79, 2, 9, 253, 68, 8, 37, 37, 22, 129, 133, 126, 33, 11, 85, 1, 0, 53, 3, 158, 28, 57,
        0, 0, 0, 129, 170, 99, 111, 109, 112, 114, 101, 115, 115, 101, 100, 164, 103, 122, 105,
        112,
    ];

    let expected = [
        mock_event("foo", "2015-09-07T01:23:04Z"),
        mock_event("bar", "2015-09-07T01:23:05Z"),
        mock_event("baz", "2015-09-07T01:23:06Z"),
    ];

    let got = decode_all(message.clone()).unwrap();

    assert_eq!(got.1, message.len());
    assert_event_data_eq!(got.0[0], expected[0]);
    assert_event_data_eq!(got.0[1], expected[1]);
    assert_event_data_eq!(got.0[2], expected[2]);
}

fn decode_all(message: Vec<u8>) -> Result<(SmallVec<[Event; 1]>, usize), DecodeError> {
    let mut buf = BytesMut::from(&message[..]);

    let mut decoder = FluentDecoder::new(LogNamespace::default());

    let (frame, byte_size) = decoder.decode(&mut buf)?.unwrap();
    Ok((frame.into(), byte_size))
}

#[test]
fn decode_incomplete_frame_requests_more_data() {
    // An array of 2 elements (`0x92`) with a tag string declaring 16 bytes
    // (`0xb0`) but only 4 bytes provided: a valid, incomplete frame. The
    // decoder should ask for more data rather than erroring.
    let partial: Vec<u8> = vec![0x92, 0xb0, b't', b'a', b'g'];
    let mut buf = BytesMut::from(&partial[..]);
    let mut decoder = FluentDecoder::new(LogNamespace::default());
    assert!(matches!(decoder.decode(&mut buf), Ok(None)));
    // The buffer is retained so more bytes can complete the frame.
    assert_eq!(buf.len(), partial.len());
}

#[test]
fn decode_oversized_frame_is_rejected() {
    // Same shape as above (a 2-element array whose string is declared far
    // larger than what has arrived), but with a decoder whose frame cap is
    // tiny. Once the buffer grows past the cap without yielding a complete
    // message, the decoder must refuse to keep buffering and signal a
    // non-recoverable error so the connection is dropped.
    let max_frame_size = 8;
    let partial: Vec<u8> = vec![0x92, 0xb0, b't', b'a', b'g', b'.', b'n', b'a', b'm', b'e'];
    assert!(partial.len() > max_frame_size);

    let mut buf = BytesMut::from(&partial[..]);
    let mut decoder = FluentDecoder {
        log_namespace: LogNamespace::default(),
        max_frame_size,
    };

    let error = match decoder.decode(&mut buf) {
        Err(error) => error,
        Ok(_) => panic!("expected FrameTooLarge error, got Ok"),
    };
    assert!(
        matches!(error, DecodeError::FrameTooLarge { size, max } if size == partial.len() && max == max_frame_size),
        "unexpected error: {error:?}"
    );
    // A frame-too-large error must terminate the connection.
    assert!(!error.can_continue());
}

#[tokio::test]
async fn ack_delivered_without_chunk() {
    let (result, output) = check_acknowledgements(EventStatus::Delivered, false).await;
    assert!(result.is_err()); // the `_` inside this error is `Elapsed`
    assert!(output.is_empty());
}

#[tokio::test]
async fn ack_delivered_with_chunk() {
    let (result, output) = check_acknowledgements(EventStatus::Delivered, true).await;
    assert_eq!(result.unwrap().unwrap(), output.len());
    let expected: Vec<u8> = vec![0x81, 0xa3, 0x61, 0x63]; // { "ack": ...
    assert_eq!(output[..expected.len()], expected);
}

#[tokio::test]
async fn ack_failed_without_chunk() {
    let (result, output) = check_acknowledgements(EventStatus::Rejected, false).await;
    assert_eq!(result.unwrap().unwrap(), output.len());
    assert!(output.is_empty());
}

#[tokio::test]
async fn ack_failed_with_chunk() {
    let (result, output) = check_acknowledgements(EventStatus::Rejected, true).await;
    assert_eq!(result.unwrap().unwrap(), output.len());
    let expected: Vec<u8> = vec![0x80]; // { }
    assert_eq!(output, expected);
}

async fn check_acknowledgements(
    status: EventStatus,
    with_chunk: bool,
) -> (Result<Result<usize, std::io::Error>, Elapsed>, Bytes) {
    trace_init();

    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (_guard, address) = next_addr();
    let source = FluentConfig {
        mode: FluentMode::Tcp(FluentTcpConfig {
            address: address.into(),
            tls: None,
            keepalive: None,
            permit_origin: None,
            receive_buffer_bytes: None,
            tls_handshake_timeout_secs: None,
            acknowledgements: true.into(),
            connection_limit: None,
        }),
        log_namespace: None,
    }
    .build(SourceContext::new_test(sender, None))
    .await
    .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let msg = uuid::Uuid::new_v4().to_string();
    let tag = uuid::Uuid::new_v4().to_string();
    let req = build_req(&tag, &[("field", &msg)], with_chunk);

    let sender = tokio::spawn(async move {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(&req).await.unwrap();

        let mut output = BytesMut::new();
        (
            timeout(Duration::from_millis(250), socket.read_buf(&mut output)).await,
            output,
        )
    });
    let events = test_util::collect_n(recv, 1).await;
    let (result, output) = sender.await.unwrap();

    assert_eq!(events.len(), 1);
    let log = events[0].as_log();
    assert_eq!(log.get(event_path!("field")).unwrap(), &msg.into());
    assert!(matches!(
        log.get(event_path!("host")).unwrap(),
        Value::Bytes(_)
    ));
    assert!(matches!(
        log.get(event_path!("timestamp")).unwrap(),
        Value::Timestamp(_)
    ));
    assert_eq!(log.get(event_path!("tag")).unwrap(), &tag.into());

    (result, output.into())
}

fn build_req(tag: &str, fields: &[(&str, &str)], with_chunk: bool) -> Vec<u8> {
    let mut record = FluentRecord::default();
    for (tag, value) in fields {
        record.insert((*tag).into(), rmpv::Value::String((*value).into()).into());
    }
    let chunk = with_chunk.then(|| BASE64_STANDARD.encode(uuid::Uuid::new_v4().as_bytes()));
    let req = FluentMessage::MessageWithOptions(
        tag.into(),
        FluentTimestamp::Unix(Utc::now()),
        record,
        FluentMessageOptions {
            chunk,
            ..Default::default()
        },
    );
    let mut buf = Vec::new();
    req.serialize(&mut Serializer::new(&mut buf)).unwrap();
    buf
}

#[test]
fn output_schema_definition_vector_namespace() {
    let config = FluentConfig {
        mode: FluentMode::Tcp(FluentTcpConfig {
            address: SocketListenAddr::SocketAddr("0.0.0.0:24224".parse().unwrap()),
            tls: None,
            keepalive: None,
            permit_origin: None,
            receive_buffer_bytes: None,
            tls_handshake_timeout_secs: None,
            acknowledgements: false.into(),
            connection_limit: None,
        }),
        log_namespace: Some(true),
    };

    let definitions = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    let expected_definition =
        Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
            .with_meaning(OwnedTargetPath::event_root(), "message")
            .with_metadata_field(
                &owned_value_path!("vector", "source_type"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(&owned_value_path!("fluent", "tag"), Kind::bytes(), None)
            .with_metadata_field(
                &owned_value_path!("fluent", "timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_metadata_field(
                &owned_value_path!("fluent", "record"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("vector", "ingest_timestamp"),
                Kind::timestamp(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("fluent", "host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_metadata_field(
                &owned_value_path!("fluent", "tls_client_metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

    assert_eq!(definitions, Some(expected_definition))
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let config = FluentConfig {
        mode: FluentMode::Tcp(FluentTcpConfig {
            address: SocketListenAddr::SocketAddr("0.0.0.0:24224".parse().unwrap()),
            tls: None,
            keepalive: None,
            permit_origin: None,
            receive_buffer_bytes: None,
            tls_handshake_timeout_secs: None,
            acknowledgements: false.into(),
            connection_limit: None,
        }),
        log_namespace: None,
    };

    let definitions = config
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()),
        [LogNamespace::Legacy],
    )
    .with_event_field(
        &owned_value_path!("message"),
        Kind::bytes(),
        Some("message"),
    )
    .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("tag"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
    .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"))
    .unknown_fields(Kind::bytes());

    assert_eq!(definitions, Some(expected_definition))
}
