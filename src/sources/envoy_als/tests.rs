use crate::{
    config::{SourceConfig, SourceContext},
    event::{Event, EventStatus, LogEvent, Value},
    sources::envoy_als::{EnvoyAlsConfig, GrpcConfig},
    test_util::{
        self,
        components::{assert_source_compliance, SOURCE_TAGS},
        next_addr,
    },
    SourceSender,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use envoy_proto::{
    envoy::config::core::v3::{
        address, node, socket_address::PortSpecifier, Address, Locality, Metadata, Node,
        SocketAddress,
    },
    envoy::data::accesslog::v3::{
        tls_properties, tls_properties::certificate_properties,
        tls_properties::certificate_properties::subject_alt_name, AccessLogCommon,
        HttpAccessLogEntry, HttpRequestProperties, HttpResponseProperties, ResponseFlags,
        TlsProperties,
    },
    envoy::service::accesslog::v3::{
        access_log_service_client::AccessLogServiceClient, stream_access_logs_message,
        StreamAccessLogsMessage,
    },
    xds::core::v3::ContextParams,
};
use futures_util::stream;
use std::collections::{BTreeMap, HashMap};
use tonic::Request;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<EnvoyAlsConfig>();
}

#[tokio::test]
async fn translate_http_log() {
    let identifier = stream_access_logs_message::Identifier {
        log_name: String::from("my-log-name"),
        node: Some(Node {
            id: String::from("my-id"),
            cluster: String::from("my-cluster"),
            metadata: Some(prost_types::Struct {
                fields: BTreeMap::from([(
                    String::from("active"),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::BoolValue(true)),
                    },
                )]),
            }),
            dynamic_parameters: HashMap::from([(
                String::from("key"),
                ContextParams {
                    params: HashMap::from([(String::from("shard"), String::from("1"))]),
                },
            )]),
            locality: Some(Locality {
                region: String::from("nyc"),
                zone: String::from("1"),
                sub_zone: String::from("a"),
            }),
            user_agent_name: String::from("envoy"),
            extensions: vec![],
            client_features: vec![String::from("my-feature")],
            user_agent_version_type: Some(node::UserAgentVersionType::UserAgentVersion(
                String::from("my-envoy-build"),
            )),
            ..Default::default()
        }),
    };
    let entry = HttpAccessLogEntry{
    common_properties: Some(AccessLogCommon{
      sample_rate: 1.0,
      downstream_remote_address: Some(Address{
        address: Some(address::Address::SocketAddress(
          SocketAddress{
            protocol: 0,
            address: String::from("222.222.222.222"),
            resolver_name: String::from(""),
            ipv4_compat: false,
            port_specifier: Some(PortSpecifier::PortValue(49964)),
          })),
      }),
      downstream_local_address: Some(Address{
        address: Some(address::Address::SocketAddress(
          SocketAddress{
            protocol: 0,
            address: String::from("111.111.111.111"),
            resolver_name: String::default(),
            ipv4_compat: false,
            port_specifier: Some(PortSpecifier::PortValue(443)),
          })),
      }),
      tls_properties: Some(TlsProperties {
        tls_version: 4,
        tls_cipher_suite: Some(4865),
        tls_sni_hostname: String::from("www.example.com"),
        local_certificate_properties : Some(
          tls_properties::CertificateProperties{
            subject: String::from("CN=www.example.com"),
            subject_alt_name: vec![certificate_properties::SubjectAltName{san: Some(subject_alt_name::San::Dns(String::from("example.com")))}],
          }
        ),
        peer_certificate_properties: None,
        tls_session_id: String::default(),
        ja3_fingerprint: String::default(),
      }),
      start_time: Some(prost_types::Timestamp{
        seconds: 1674989598,
        nanos: 201850000
      }),
      time_to_last_rx_byte: Some(prost_types::Duration{seconds: 0, nanos: 81191}),
      time_to_first_upstream_tx_byte: Some(prost_types::Duration{seconds: 0, nanos: 278830}),
      time_to_last_upstream_tx_byte: Some(prost_types::Duration{seconds: 0, nanos: 333362}),
      time_to_first_upstream_rx_byte: Some(prost_types::Duration{seconds: 0, nanos: 930717}),
      time_to_last_upstream_rx_byte: Some(prost_types::Duration{seconds: 0, nanos: 1011397}),
      time_to_first_downstream_tx_byte: Some(prost_types::Duration{seconds: 0, nanos: 957737}),
      time_to_last_downstream_tx_byte: Some(prost_types::Duration{seconds: 0, nanos: 1021696}),
      upstream_remote_address: Some(Address{
        address: Some(address::Address::SocketAddress(
          SocketAddress{
            protocol: 0,
            address: String::from("10.10.10.11"),
            resolver_name: String::default(),
            ipv4_compat: false,
            port_specifier: Some(PortSpecifier::PortValue(80)),
          })),
      }),
      upstream_local_address: Some(Address{
        address: Some(address::Address::SocketAddress(
          SocketAddress{
            protocol: 0,
            address: String::from("10.10.10.10"),
            resolver_name: String::default(),
            ipv4_compat: false,
            port_specifier: Some(PortSpecifier::PortValue(38296)),
        })),
      }),
      upstream_cluster: String::from("backends"),
      response_flags: Some(ResponseFlags::default()),
      metadata: Some(Metadata{
        filter_metadata: HashMap::from(
          [(
            String::from("envoy.filters.http.cdn"),
            prost_types::Struct{
              fields: BTreeMap::from([(
                String::from("cached"),
                prost_types::Value{kind: Some(prost_types::value::Kind::BoolValue(true))},
              )]),
            },
          )],
        ),
        typed_filter_metadata: std::collections::HashMap::default(),
      }),
      upstream_transport_failure_reason: String::default(),
      route_name: String::from("my-backend-route"),
      downstream_direct_remote_address: Some(Address{
        address: Some(address::Address::SocketAddress(
          SocketAddress{
            protocol: 0,
            address: String::from("222.222.222.222"),
            resolver_name: String::from(""),
            ipv4_compat: false,
            port_specifier: Some(PortSpecifier::PortValue(49964)),
          })),
      }),
      filter_state_objects: HashMap::from(
        [(
          String::from("my-test-object"),
          prost_types::Any{
            type_url: String::from("type.googleapis.com/google.protobuf.Duration"),
            value: String::from("1.212s").into_bytes(),
          },
        )]
      ),
      custom_tags: HashMap::from([(String::from("tag1"), String::from("val1")), (String::from("tag2"), String::from("val2"))]),
      duration: Some(prost_types::Duration{seconds: 0, nanos: 1021896}),
      upstream_request_attempt_count: 1,
      connection_termination_details: String::default(),
    }),
    protocol_version: 2,
    request: Some(HttpRequestProperties{
      request_method: 1,
      scheme: String::from("https"),
      authority: String::from("www.example.com"),
      port: None,
      path: String::from("/"),
      user_agent: String::from("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/105.0.0.0 Safari/537.36"),
      referer: String::default(),
      forwarded_for: String::from("222.222.222.222"),
      request_id: String::from("b41a828c-873b-4649-9b6e-a17b9af05c5a"),
      original_path: String::default(),
      request_headers_bytes: 213,
      request_body_bytes: 200,
      request_headers: HashMap::from([(String::from("accept"), String::from("*/*"))])
    }),
    response: Some(HttpResponseProperties{
      response_code: Some(200),
      response_headers_bytes: 300,
      response_body_bytes: 612,
      response_headers: HashMap::from([(String::from("content-type"), String::from("text/html"))]),
      response_trailers: HashMap::from([(String::from("expires"), String::from("Wed, 1 Jan 2023 08:00:00 GMT"))]),
      response_code_details: String::from("via_upstream"),
    }),
  };

    assert_source_compliance(&SOURCE_TAGS, async {
    let grpc_addr = next_addr();

    let source = EnvoyAlsConfig {
        grpc: GrpcConfig {
            address: grpc_addr,
            tls: Default::default(),
        },
    };
    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let server = source
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
    tokio::spawn(server);
    test_util::wait_for_tcp(grpc_addr).await;

    let logs = vec![StreamAccessLogsMessage{
      identifier: Some(identifier),
      log_entries: Some(stream_access_logs_message::LogEntries::HttpLogs(stream_access_logs_message::HttpAccessLogEntries{log_entry: vec![entry]})),
    }];
    let req = Request::new(stream::iter(logs));

    // send request via grpc client
    let mut client = AccessLogServiceClient::connect(format!("http://{}", grpc_addr))
        .await
        .unwrap();
    let _ = client.stream_access_logs(req).await;
    let mut output = test_util::collect_ready(recv).await;
    assert_eq!(output.len(), 1);
    // we just send one, so only one output
    let actual_event = output.pop().unwrap();
    let expect_map =  BTreeMap::from(
      [
        (String::from("identifier"),Value::Object(
          BTreeMap::from(
            [
              (String::from("log_name"), Value::Bytes("my-log-name".into())),
              (String::from("node"), Value::Object(
                BTreeMap::from([
                  (String::from("id"), Value::Bytes("my-id".into())),
                  (String::from("cluster"), Value::Bytes("my-cluster".into())),
                  (String::from("metadata"), Value::Object(BTreeMap::from([
                      (String::from("active"), Value::Boolean(true)),
                  ]))),
                  (String::from("dynamic_parameters"), Value::Object(BTreeMap::from([
                    (String::from("key"), Value::Object(BTreeMap::from([
                      (String::from("params"), Value::Object(BTreeMap::from([
                        (String::from("shard"), Value::Bytes("1".into())),
                      ]))),
                    ]))),
                  ]))),
                  (String::from("locality"), Value::Object(BTreeMap::from([
                    (String::from("region"), Value::Bytes("nyc".into())),
                    (String::from("zone"), Value::Bytes("1".into())),
                    (String::from("sub_zone"), Value::Bytes("a".into())),
                  ]))),
                  (String::from("user_agent_name"), Value::Bytes("envoy".into())),
                  (String::from("client_features"), Value::Array(vec![Value::Bytes("my-feature".into())])),
                  (String::from("user_agent_version_type"), Value::Object(BTreeMap::from([
                    (String::from("user_agent_version"), Value::Bytes("my-envoy-build".into())),
                  ]))),
                ]),
              )),
            ]
          )
        )),
        (String::from("http_log"),Value::Object(
          BTreeMap::from(
            [
              (String::from("common_properties"), Value::Object(
                BTreeMap::from(
                  [
                    (String::from("connection_termination_details"), Value::Bytes("".into())),
                    (String::from("custom_tags"), Value::Object(
                      BTreeMap::from([
                        (String::from("tag1"), Value::Bytes("val1".into())),
                        (String::from("tag2"), Value::Bytes("val2".into())),
                      ]),
                    )),
                    (String::from("downstream_direct_remote_address"),
                      Value::Object(BTreeMap::from([
                        (String::from("socket_address"), Value::Object(BTreeMap::from([
                          (String::from("address"), Value::Bytes("222.222.222.222".into())),
                          (String::from("ipv4_compat"), Value::Boolean(false)),
                          (String::from("port_specifier"), Value::Object(BTreeMap::from([
                            (String::from("port_value"), Value::Integer(49964)),
                          ]))),
                          (String::from("protocol"), Value::Bytes("TCP".into())),
                          (String::from("resolver_name"), Value::Bytes("".into())),
                        ]))),
                      ])),
                    ),
                    (String::from("downstream_local_address"),
                      Value::Object(BTreeMap::from([
                        (String::from("socket_address"), Value::Object(BTreeMap::from([
                          (String::from("address"), Value::Bytes("111.111.111.111".into())),
                          (String::from("ipv4_compat"), Value::Boolean(false)),
                          (String::from("port_specifier"), Value::Object(BTreeMap::from([
                            (String::from("port_value"), Value::Integer(443)),
                          ]))),
                          (String::from("protocol"), Value::Bytes("TCP".into())),
                          (String::from("resolver_name"), Value::Bytes("".into())),
                        ]))),
                      ])),
                    ),
                    (String::from("downstream_remote_address"),
                      Value::Object(BTreeMap::from([
                        (String::from("socket_address"), Value::Object(BTreeMap::from([
                          (String::from("address"), Value::Bytes("222.222.222.222".into())),
                          (String::from("ipv4_compat"), Value::Boolean(false)),
                          (String::from("port_specifier"), Value::Object(BTreeMap::from([
                            (String::from("port_value"), Value::Integer(49964)),
                          ]))),
                          (String::from("protocol"), Value::Bytes("TCP".into())),
                          (String::from("resolver_name"), Value::Bytes("".into())),
                        ]))),
                      ])),
                    ),
                    (String::from("duration_ns"), Value::Integer(1021896)),
                    (String::from("filter_state_objects"), Value::Object(
                      BTreeMap::from([
                        (String::from("my-test-object"), Value::Object(BTreeMap::from([
                          (String::from("type_url"), Value::Bytes("type.googleapis.com/google.protobuf.Duration".into())),
                          (String::from("value"), Value::Bytes("1.212s".into())),
                        ]))),
                      ]),
                    )),
                    (String::from("metadata"), Value::Object(
                      BTreeMap::from([
                        (String::from("filter_metadata"), Value::Object(BTreeMap::from([
                          (String::from("envoy.filters.http.cdn"), Value::Object(BTreeMap::from([
                            (String::from("cached"), Value::Boolean(true)),
                          ]))),
                        ]))),
                        (String::from("typed_filter_metadata"), Value::Object(BTreeMap::new())),
                      ]),
                    )),
                    (String::from("response_flags"), Value::Object(
                      BTreeMap::from([
                        (String::from("delay_injected"), Value::Boolean(false)),
                        (String::from("dns_resolution_failure"), Value::Boolean(false)),
                        (String::from("downstream_connection_termination"), Value::Boolean(false)),
                        (String::from("downstream_protocol_error"), Value::Boolean(false)),
                        (String::from("duration_timeout"), Value::Boolean(false)),
                        (String::from("failed_local_healthcheck"), Value::Boolean(false)),
                        (String::from("fault_injected"), Value::Boolean(false)),
                        (String::from("invalid_envoy_request_headers"), Value::Boolean(false)),
                        (String::from("local_reset"), Value::Boolean(false)),
                        (String::from("no_cluster_found"), Value::Boolean(false)),
                        (String::from("no_filter_config_found"), Value::Boolean(false)),
                        (String::from("no_healthy_upstream"), Value::Boolean(false)),
                        (String::from("no_route_found"), Value::Boolean(false)),
                        (String::from("overload_manager"), Value::Boolean(false)),
                        (String::from("rate_limit_service_error"), Value::Boolean(false)),
                        (String::from("rate_limited"), Value::Boolean(false)),
                        (String::from("response_from_cache_filter"), Value::Boolean(false)),
                        (String::from("stream_idle_timeout"), Value::Boolean(false)),
                        (String::from("upstream_connection_failure"), Value::Boolean(false)),
                        (String::from("upstream_connection_termination"), Value::Boolean(false)),
                        (String::from("upstream_max_stream_duration_reached"), Value::Boolean(false)),
                        (String::from("upstream_overflow"), Value::Boolean(false)),
                        (String::from("upstream_protocol_error"), Value::Boolean(false)),
                        (String::from("upstream_remote_reset"), Value::Boolean(false)),
                        (String::from("upstream_request_timeout"), Value::Boolean(false)),
                        (String::from("upstream_retry_limit_exceeded"), Value::Boolean(false)),
                      ]),
                    )),
                    (String::from("route_name"), Value::Bytes("my-backend-route".into())),
                    (String::from("start_time"), Value::Timestamp(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(1674989598, 201850000), Utc))),
                    (String::from("time_to_first_downstream_tx_byte_ns"), Value::Integer(957737)),
                    (String::from("time_to_first_upstream_rx_byte_ns"), Value::Integer(930717)),
                    (String::from("time_to_first_upstream_tx_byte_ns"), Value::Integer(278830)),
                    (String::from("time_to_last_downstream_tx_byte_ns"), Value::Integer(1021696)),
                    (String::from("time_to_last_rx_byte_ns"), Value::Integer(81191)),
                    (String::from("time_to_last_upstream_rx_byte_ns"), Value::Integer(1011397)),
                    (String::from("time_to_last_upstream_tx_byte_ns"), Value::Integer(333362)),
                    (String::from("tls_properties"), Value::Object(
                      BTreeMap::from([
                        (String::from("ja3_fingerprint"), Value::Bytes("".into())),
                        (String::from("local_certificate_properties"), Value::Object(BTreeMap::from([
                          (String::from("subject"), Value::Bytes("CN=www.example.com".into())),
                          (String::from("subject_alt_name"), Value::Array(vec![
                            Value::Object(BTreeMap::from([
                              (String::from("san"), Value::Object(BTreeMap::from([
                                (String::from("dns"), Value::Bytes("example.com".into())),
                              ]))),
                            ]))
                          ])),
                        ]))),
                        (String::from("tls_cipher_suite"), Value::Integer(4865)),
                        (String::from("tls_session_id"), Value::Bytes("".into())),
                        (String::from("tls_sni_hostname"), Value::Bytes("www.example.com".into())),
                        (String::from("tls_version"), Value::Object(BTreeMap::from([
                          (String::from("tls_version"), Value::Bytes("TLSv1_3".into())),
                        ]))),
                      ]),
                    )),
                    (String::from("upstream_cluster"), Value::Bytes("backends".into())),
                    (String::from("upstream_local_address"),
                      Value::Object(BTreeMap::from([
                        (String::from("socket_address"), Value::Object(BTreeMap::from([
                          (String::from("address"), Value::Bytes("10.10.10.10".into())),
                          (String::from("ipv4_compat"), Value::Boolean(false)),
                          (String::from("port_specifier"), Value::Object(BTreeMap::from([
                            (String::from("port_value"), Value::Integer(38296)),
                          ]))),
                          (String::from("protocol"), Value::Bytes("TCP".into())),
                          (String::from("resolver_name"), Value::Bytes("".into())),
                        ]))),
                      ])),
                    ),
                    (String::from("upstream_remote_address"),
                      Value::Object(BTreeMap::from([
                        (String::from("socket_address"), Value::Object(BTreeMap::from([
                          (String::from("address"), Value::Bytes("10.10.10.11".into())),
                          (String::from("ipv4_compat"), Value::Boolean(false)),
                          (String::from("port_specifier"), Value::Object(BTreeMap::from([
                            (String::from("port_value"), Value::Integer(80)),
                          ]))),
                          (String::from("protocol"), Value::Bytes("TCP".into())),
                          (String::from("resolver_name"), Value::Bytes("".into())),
                        ]))),
                      ])),
                    ),
                    (String::from("upstream_request_attempt_count"), Value::Integer(1)),
                    (String::from("upstream_transport_failure_reason"), Value::Bytes("".into())),
                  ]
                )
              )),
              (String::from("protocol_version"), Value::Bytes("HTTP11".into())),
              (String::from("request"), Value::Object(
                BTreeMap::from(
                  [
                    (String::from("authority"), Value::Bytes("www.example.com".into())),
                    (String::from("forwarded_for"), Value::Bytes("222.222.222.222".into())),
                    (String::from("original_path"), Value::Bytes("".into())),
                    (String::from("path"), Value::Bytes("/".into())),
                    (String::from("referer"), Value::Bytes("".into())),
                    (String::from("request_body_bytes"), Value::Integer(200)),
                    (String::from("request_headers"), Value::Object(
                      BTreeMap::from([(String::from("accept"), Value::Bytes("*/*".into()))]),
                    )),
                    (String::from("request_headers_bytes"), Value::Integer(213)),
                    (String::from("request_id"), Value::Bytes("b41a828c-873b-4649-9b6e-a17b9af05c5a".into())),
                    (String::from("request_method"), Value::Bytes("GET".into())),
                    (String::from("scheme"), Value::Bytes("https".into())),
                    (String::from("user_agent"), Value::Bytes("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/105.0.0.0 Safari/537.36".into())),
                  ]
                )
              )),
              (String::from("response"), Value::Object(
                BTreeMap::from(
                  [
                    (String::from("response_body_bytes"), Value::Integer(612)),
                    (String::from("response_code"), Value::Integer(200)),
                    (String::from("response_code_details"), Value::Bytes("via_upstream".into())),
                    (String::from("response_headers"), Value::Object(
                      BTreeMap::from([(String::from("content-type"), Value::Bytes("text/html".into()))]),
                    )),
                    (String::from("response_headers_bytes"), Value::Integer(300)),
                    (String::from("response_trailers"), Value::Object(
                      BTreeMap::from([(String::from("expires"), Value::Bytes("Wed, 1 Jan 2023 08:00:00 GMT".into()))]),
                    )),
                  ]
                )
              )),
            ]
          ),
        ))
      ]
    );
    let expect_event = Event::from(LogEvent::from(expect_map));
    assert_eq!(actual_event, expect_event);

  })
  .await;
}
