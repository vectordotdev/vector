#![allow(clippy::print_stdout)] //tests

use std::{
    io::{Cursor, Read},
    net::SocketAddr,
    sync::LazyLock,
};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{DateTime, SubsecRound, Utc};
use flate2::read::GzEncoder;
use futures::Stream;
use similar_asserts::assert_eq;
use vector_lib::{assert_event_data_eq, lookup::path};
use vrl::{value, value::KeyString, value::ObjectMap, value::Value};

use super::*;
use crate::{
    SourceSender,
    event::{Event, EventStatus},
    log_event,
    test_util::{
        addr::{PortGuard, next_addr},
        collect_n,
        components::{SOURCE_TAGS, assert_source_compliance},
        wait_for_tcp,
    },
};

const SOURCE_ARN: &str = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
const REQUEST_ID: &str = "e17265d6-97af-4938-982e-90d5614c4242";
// example CloudWatch Logs subscription event
const RECORD: &str = r#"
        {
            "messageType": "DATA_MESSAGE",
            "owner": "071959437513",
            "logGroup": "/jesse/test",
            "logStream": "test",
            "subscriptionFilters": ["Destination"],
            "logEvents": [
                {
                    "id": "35683658089614582423604394983260738922885519999578275840",
                    "timestamp": 1600110569039,
                    "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
                },
                {
                    "id": "35683658089659183914001456229543810359430816722590236673",
                    "timestamp": 1600110569041,
                    "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
                }
            ]
        }
    "#;

const COMMON_ATTRIBUTES: &str =
    r#"{ "commonAttributes": { "environment": "testing", "application_group": "tymur_test" } }"#;

static COMMON_ATTRIBUTES_MAP: LazyLock<ObjectMap> = LazyLock::new(|| {
    ObjectMap::from_iter([
        (
            KeyString::from("environment"),
            Value::Bytes("testing".into()),
        ),
        (
            KeyString::from("application_group"),
            Value::Bytes("tymur_test".into()),
        ),
    ])
});

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AwsKinesisFirehoseConfig>();
}

async fn source(
    access_key: Option<SensitiveString>,
    access_keys: Option<Vec<SensitiveString>>,
    store_access_key: bool,
    record_compression: Compression,
    delivered: bool,
    log_namespace: bool,
    common_attributes: Vec<String>,
) -> (impl Stream<Item = Event> + Unpin, SocketAddr, PortGuard) {
    use EventStatus::*;
    let status = if delivered { Delivered } else { Rejected };
    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (_guard, address) = next_addr();
    let cx = SourceContext::new_test(sender, None);
    tokio::spawn(async move {
        AwsKinesisFirehoseConfig {
            address,
            tls: None,
            access_key,
            access_keys,
            store_access_key,
            record_compression,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: true.into(),
            log_namespace: Some(log_namespace),
            keepalive: Default::default(),
            common_attributes,
        }
        .build(cx)
        .await
        .unwrap()
        .await
        .unwrap()
    });
    // Wait for the component to bind to the port
    wait_for_tcp(address).await;
    (recv, address, _guard)
}

/// Sends the body to the address with the appropriate Firehose headers
///
/// https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html
async fn send(
    address: SocketAddr,
    timestamp: DateTime<Utc>,
    records: Vec<&[u8]>,
    key: Option<&str>,
    gzip: bool,
    record_compression: Compression,
    common_attributes: Option<&str>,
) -> reqwest::Result<reqwest::Response> {
    let request = models::FirehoseRequest {
        access_key: key.map(|s| s.to_string()),
        request_id: REQUEST_ID.to_string(),
        timestamp,
        records: records
            .into_iter()
            .map(|record| models::EncodedFirehoseRecord {
                data: encode_record(record, record_compression).unwrap(),
            })
            .collect(),
    };

    let mut builder = reqwest::Client::new()
        .post(format!("http://{address}"))
        .header("host", address.to_string())
        .header(
            "x-amzn-trace-id",
            "Root=1-5f5fbf1c-877c68cace58bea222ddbeec",
        )
        .header("x-amz-firehose-protocol-version", "1.0")
        .header("x-amz-firehose-request-id", REQUEST_ID.to_string())
        .header("x-amz-firehose-source-arn", SOURCE_ARN.to_string())
        .header("user-agent", "Amazon Kinesis Data Firehose Agent/1.0")
        .header("content-type", "application/json");

    if let Some(key) = key {
        builder = builder.header("x-amz-firehose-access-key", key);
    }

    if let Some(common_attributes) = common_attributes {
        builder = builder.header("x-amz-firehose-common-attributes", common_attributes)
    }

    if gzip {
        let mut gz = GzEncoder::new(
            Cursor::new(serde_json::to_vec(&request).unwrap()),
            flate2::Compression::fast(),
        );
        let mut buffer = Vec::new();
        gz.read_to_end(&mut buffer).unwrap();
        builder = builder.header("content-encoding", "gzip").body(buffer);
    } else {
        builder = builder.json(&request);
    }

    builder.send().await
}

async fn spawn_send(
    address: SocketAddr,
    timestamp: DateTime<Utc>,
    records: Vec<&'static [u8]>,
    key: Option<&'static str>,
    gzip: bool,
    record_compression: Compression,
    common_attributes: Option<&'static str>,
) -> tokio::task::JoinHandle<reqwest::Result<reqwest::Response>> {
    tokio::spawn(async move {
        send(
            address,
            timestamp,
            records,
            key,
            gzip,
            record_compression,
            common_attributes,
        )
        .await
    })
}

/// Encodes record data to mach AWS's representation: base64 encoded with an additional
/// compression
fn encode_record(record: &[u8], compression: Compression) -> std::io::Result<String> {
    let compressed = match compression {
        Compression::Auto => panic!("cannot encode records as Auto"),
        Compression::Gzip => {
            let mut buffer = Vec::new();
            if !record.is_empty() {
                let mut gz = GzEncoder::new(record, flate2::Compression::fast());
                gz.read_to_end(&mut buffer)?;
            }
            buffer
        }
        Compression::None => record.to_vec(),
    };

    Ok(BASE64_STANDARD.encode(compressed))
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_legacy_namespace() {
    let gzipped_record = {
        let mut buf = Vec::new();
        let mut gz = GzEncoder::new(RECORD.as_bytes(), flate2::Compression::fast());
        gz.read_to_end(&mut buf).unwrap();
        buf
    };

    for (source_record_compression, record_compression, success, record, expected) in [
        (
            Compression::Auto,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Auto,
            Compression::None,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::None,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            gzipped_record,
        ),
        (
            Compression::None,
            Compression::None,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::None,
            false,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::Gzip,
            true,
            "".as_bytes(),
            Vec::new(),
        ),
    ] {
        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            source_record_compression,
            true,
            false,
            vec![],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![record],
            None,
            false,
            record_compression,
            None,
        )
        .await;

        if success {
            let events = collect_n(rx, 1).await;

            let res = res.await.unwrap().unwrap();
            assert_eq!(200, res.status().as_u16());

            assert_event_data_eq!(
                events,
                vec![log_event! {
                    "source_type" => Bytes::from("aws_kinesis_firehose"),
                    "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                    "message" => Bytes::from(expected),
                    "request_id" => REQUEST_ID,
                    "source_arn" => SOURCE_ARN,
                },]
            );

            let response: models::FirehoseResponse = res.json().await.unwrap();
            assert_eq!(response.request_id, REQUEST_ID);
        } else {
            let res = res.await.unwrap().unwrap();
            assert_eq!(400, res.status().as_u16());
        }
    }
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_vector_namespace() {
    let gzipped_record = {
        let mut buf = Vec::new();
        let mut gz = GzEncoder::new(RECORD.as_bytes(), flate2::Compression::fast());
        gz.read_to_end(&mut buf).unwrap();
        buf
    };

    for (source_record_compression, record_compression, success, record, expected) in [
        (
            Compression::Auto,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Auto,
            Compression::None,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::None,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            gzipped_record,
        ),
        (
            Compression::None,
            Compression::None,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::Gzip,
            true,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::None,
            false,
            RECORD.as_bytes(),
            RECORD.as_bytes().to_owned(),
        ),
        (
            Compression::Gzip,
            Compression::Gzip,
            true,
            "".as_bytes(),
            Vec::new(),
        ),
    ] {
        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            source_record_compression,
            true,
            true,
            vec![],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![record],
            None,
            false,
            record_compression,
            None,
        )
        .await;

        if success {
            let events = collect_n(rx, 1).await;

            let res = res.await.unwrap().unwrap();
            assert_eq!(200, res.status().as_u16());

            for event in events {
                let log = event.as_log();
                let meta = log.metadata();

                // event data, currently assumes default bytes deserializer
                assert_eq!(log.value(), &value!(Bytes::from(expected.to_owned())));

                // vector metadata
                assert_eq!(
                    meta.value().get(path!("vector", "source_type")).unwrap(),
                    &value!("aws_kinesis_firehose")
                );
                assert!(
                    meta.value()
                        .get(path!("vector", "ingest_timestamp"))
                        .unwrap()
                        .is_timestamp()
                );

                // source metadata
                assert_eq!(
                    meta.value()
                        .get(path!("aws_kinesis_firehose", "request_id"))
                        .unwrap(),
                    &value!(REQUEST_ID)
                );
                assert_eq!(
                    meta.value()
                        .get(path!("aws_kinesis_firehose", "source_arn"))
                        .unwrap(),
                    &value!(SOURCE_ARN)
                );
                assert_eq!(
                    meta.value()
                        .get(path!("aws_kinesis_firehose", "timestamp"))
                        .unwrap(),
                    &value!(timestamp.trunc_subsecs(3))
                );
                assert!(
                    meta.value()
                        .get(path!("aws_kinesis_firehose", "common_attributes"))
                        .is_none()
                );
            }

            let response: models::FirehoseResponse = res.json().await.unwrap();
            assert_eq!(response.request_id, REQUEST_ID);
        } else {
            let res = res.await.unwrap().unwrap();
            assert_eq!(400, res.status().as_u16());
        }
    }
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_gzip_request() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let (rx, addr, _guard) =
            source(None, None, false, Default::default(), true, false, vec![]).await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            None,
        )
        .await;

        let events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        assert_event_data_eq!(
            events,
            vec![log_event! {
                "source_type" => Bytes::from("aws_kinesis_firehose"),
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> RECORD,
                "request_id" => REQUEST_ID,
                "source_arn" => SOURCE_ARN,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_wildcard_common_attributes_legacy_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            Default::default(),
            true,
            false,
            vec!["*".to_string()],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            Some(COMMON_ATTRIBUTES),
        )
        .await;

        let events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        assert_event_data_eq!(
            events,
            vec![log_event! {
                "source_type" => Bytes::from("aws_kinesis_firehose"),
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> RECORD,
                "request_id" => REQUEST_ID,
                "source_arn" => SOURCE_ARN,
                "common_attributes" => COMMON_ATTRIBUTES_MAP.clone(),
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_wildcard_common_attributes_vector_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            Default::default(),
            true,
            true,
            vec!["*".to_string()],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            Some(COMMON_ATTRIBUTES),
        )
        .await;

        let mut events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        let event = events.pop().unwrap();
        let log = event.as_log();
        let meta = log.metadata();

        // event data, currently assumes default bytes deserializer
        assert_eq!(log.value(), &value!(Bytes::from(RECORD.to_owned())));

        // vector metadata
        assert_eq!(
            meta.value().get(path!("vector", "source_type")).unwrap(),
            &value!("aws_kinesis_firehose")
        );
        assert!(
            meta.value()
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp()
        );

        // source metadata
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "request_id"))
                .unwrap(),
            &value!(REQUEST_ID)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "source_arn"))
                .unwrap(),
            &value!(SOURCE_ARN)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "timestamp"))
                .unwrap(),
            &value!(timestamp.trunc_subsecs(3))
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "common_attributes"))
                .unwrap(),
            &value!(COMMON_ATTRIBUTES_MAP.clone())
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_common_attributes_legacy_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let mut expected_common_attributes = ObjectMap::new();
        expected_common_attributes.insert(
            KeyString::from("environment"),
            COMMON_ATTRIBUTES_MAP["environment"].clone(),
        );
        expected_common_attributes.insert(KeyString::from("absent_attribute"), Value::Null);

        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            Default::default(),
            true,
            false,
            vec!["environment".to_string(), "absent_attribute".to_string()],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            Some(COMMON_ATTRIBUTES),
        )
        .await;

        let events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        assert_event_data_eq!(
            events,
            vec![log_event! {
                "source_type" => Bytes::from("aws_kinesis_firehose"),
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> RECORD,
                "request_id" => REQUEST_ID,
                "source_arn" => SOURCE_ARN,
                "common_attributes" => expected_common_attributes,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

#[tokio::test]
async fn aws_kinesis_firehose_forwards_events_common_attributes_vector_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let mut expected_common_attributes = ObjectMap::new();
        expected_common_attributes.insert(
            KeyString::from("environment"),
            COMMON_ATTRIBUTES_MAP["environment"].clone(),
        );
        expected_common_attributes.insert(KeyString::from("absent_attribute"), Value::Null);

        let (rx, addr, _guard) = source(
            None,
            None,
            false,
            Default::default(),
            true,
            true,
            vec!["environment".to_string(), "absent_attribute".to_string()],
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            Some(COMMON_ATTRIBUTES),
        )
        .await;

        let mut events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        let event = events.pop().unwrap();
        let log = event.as_log();
        let meta = log.metadata();

        // event data, currently assumes default bytes deserializer
        assert_eq!(log.value(), &value!(Bytes::from(RECORD.to_owned())));

        // vector metadata
        assert_eq!(
            meta.value().get(path!("vector", "source_type")).unwrap(),
            &value!("aws_kinesis_firehose")
        );
        assert!(
            meta.value()
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp()
        );

        // source metadata
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "request_id"))
                .unwrap(),
            &value!(REQUEST_ID)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "source_arn"))
                .unwrap(),
            &value!(SOURCE_ARN)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "timestamp"))
                .unwrap(),
            &value!(timestamp.trunc_subsecs(3))
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "common_attributes"))
                .unwrap(),
            &value!(expected_common_attributes)
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

// Test there is no regression for existing setups and non-AWS test senders that previously
// ignored X-Amz-Firehose-Common-Attributes header after firehose common attributes were introduced
// (https://github.com/vectordotdev/vector/pull/24914#discussion_r3024341032)
#[tokio::test]
async fn aws_kinesis_firehose_ignores_malformed_common_attributes_if_none_configured() {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let (rx, addr, _guard) =
            source(None, None, false, Default::default(), true, true, vec![]).await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            true,
            Compression::None,
            Some("malformed-common-attributes"),
        )
        .await;

        let mut events = collect_n(rx, 1).await;
        let res = res.await.unwrap().unwrap();
        assert_eq!(200, res.status().as_u16());

        let event = events.pop().unwrap();
        let log = event.as_log();
        let meta = log.metadata();

        // event data, currently assumes default bytes deserializer
        assert_eq!(log.value(), &value!(Bytes::from(RECORD.to_owned())));

        // vector metadata
        assert_eq!(
            meta.value().get(path!("vector", "source_type")).unwrap(),
            &value!("aws_kinesis_firehose")
        );
        assert!(
            meta.value()
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp()
        );

        // source metadata
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "request_id"))
                .unwrap(),
            &value!(REQUEST_ID)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "source_arn"))
                .unwrap(),
            &value!(SOURCE_ARN)
        );
        assert_eq!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "timestamp"))
                .unwrap(),
            &value!(timestamp.trunc_subsecs(3))
        );
        assert!(
            meta.value()
                .get(path!("aws_kinesis_firehose", "common_attributes"))
                .is_none()
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    })
    .await;
}

#[tokio::test]
async fn aws_kinesis_firehose_rejects_bad_access_key() {
    let (_rx, addr, _guard) = source(
        Some("an access key".to_string().into()),
        Some(vec!["an access key in list".to_string().into()]),
        Default::default(),
        Default::default(),
        true,
        false,
        vec![],
    )
    .await;

    let res = send(
        addr,
        Utc::now(),
        vec![],
        Some("bad access key"),
        false,
        Compression::None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(401, res.status().as_u16());

    let response: models::FirehoseResponse = res.json().await.unwrap();
    assert_eq!(response.request_id, REQUEST_ID);
}

#[tokio::test]
async fn aws_kinesis_firehose_rejects_bad_access_key_from_list() {
    let (_rx, addr, _guard) = source(
        None,
        Some(vec!["an access key in list".to_string().into()]),
        Default::default(),
        Default::default(),
        true,
        false,
        vec![],
    )
    .await;

    let res = send(
        addr,
        Utc::now(),
        vec![],
        Some("bad access key"),
        false,
        Compression::None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(401, res.status().as_u16());

    let response: models::FirehoseResponse = res.json().await.unwrap();
    assert_eq!(response.request_id, REQUEST_ID);
}

#[tokio::test]
async fn aws_kinesis_firehose_accepts_merged_access_keys() {
    let valid_access_key = SensitiveString::from(String::from("an access key in list"));

    let (_rx, addr, _guard) = source(
        Some(valid_access_key.clone()),
        Some(vec!["valid access key 2".to_string().into()]),
        Default::default(),
        Default::default(),
        true,
        false,
        vec![],
    )
    .await;

    let res = send(
        addr,
        Utc::now(),
        vec![],
        Some(valid_access_key.clone().inner()),
        false,
        Compression::None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(200, res.status().as_u16());

    let response: models::FirehoseResponse = res.json().await.unwrap();
    assert_eq!(response.request_id, REQUEST_ID);
}

#[tokio::test]
async fn aws_kinesis_firehose_accepts_access_keys_from_list() {
    let valid_access_key = "an access key in list".to_string();

    let (_rx, addr, _guard) = source(
        None,
        Some(vec![
            valid_access_key.clone().into(),
            "valid access key 2".to_string().into(),
        ]),
        Default::default(),
        Default::default(),
        true,
        false,
        vec![],
    )
    .await;

    let res = send(
        addr,
        Utc::now(),
        vec![],
        Some(&valid_access_key),
        false,
        Compression::None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(200, res.status().as_u16());

    let response: models::FirehoseResponse = res.json().await.unwrap();
    assert_eq!(response.request_id, REQUEST_ID);
}

#[tokio::test]
async fn handles_acknowledgement_failure() {
    let expected = RECORD.as_bytes().to_owned();

    let (rx, addr, _guard) =
        source(None, None, false, Compression::None, false, false, vec![]).await;

    let timestamp: DateTime<Utc> = Utc::now();

    let res = spawn_send(
        addr,
        timestamp,
        vec![RECORD.as_bytes()],
        None,
        false,
        Compression::None,
        None,
    )
    .await;

    let events = collect_n(rx, 1).await;

    let res = res.await.unwrap().unwrap();
    assert_eq!(406, res.status().as_u16());

    assert_event_data_eq!(
        events,
        vec![log_event! {
            "source_type" => Bytes::from("aws_kinesis_firehose"),
            "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
            "message"=> Bytes::from(expected),
            "request_id" => REQUEST_ID,
            "source_arn" => SOURCE_ARN,
        },]
    );

    let response: models::FirehoseResponse = res.json().await.unwrap();
    assert_eq!(response.request_id, REQUEST_ID);
}

#[tokio::test]
async fn event_access_key_passthrough_enabled() {
    let (rx, address, _guard) = source(
        None,
        Some(vec!["an access key".to_string().into()]),
        true,
        Default::default(),
        true,
        true,
        vec![],
    )
    .await;

    let timestamp: DateTime<Utc> = Utc::now();

    spawn_send(
        address,
        timestamp,
        vec![RECORD.as_bytes()],
        Some("an access key"),
        false,
        Compression::None,
        None,
    )
    .await;

    let events = collect_n(rx, 1).await;
    let access_key = events[0]
        .metadata()
        .secrets()
        .get("aws_kinesis_firehose_access_key")
        .unwrap();
    assert_eq!(access_key.to_string(), "an access key".to_string());
}

#[tokio::test]
async fn no_authorization_access_key_passthrough_enabled() {
    let (rx, address, _guard) =
        source(None, None, true, Default::default(), true, true, vec![]).await;

    let timestamp: DateTime<Utc> = Utc::now();

    spawn_send(
        address,
        timestamp,
        vec![RECORD.as_bytes()],
        None,
        false,
        Compression::None,
        None,
    )
    .await;

    let events = collect_n(rx, 1).await;

    assert!(
        events[0]
            .metadata()
            .secrets()
            .get("aws_kinesis_firehose_access_key")
            .is_none()
    );
}
