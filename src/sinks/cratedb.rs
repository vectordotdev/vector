use crate::{
    buffers::Acker,
    event::{unflatten::MapValue, Event},
    sinks::util::{
        http::{HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        Batch, BatchConfig, BatchServiceSink, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{stream, Future, Sink};
use headers::HeaderMapExt;
use http::Uri;
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct CrateDBSinkConfig {
    pub host: String,
    pub schema: Option<String>,
    pub table: Option<String>,
    pub columns: Option<Vec<String>>,
    pub keys: Option<Vec<String>>,
    #[serde(flatten)]
    pub basic_auth: Option<BasicAuth>,
    #[serde(default, flatten)]
    pub batch: BatchConfig,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    user: String,
    password: String,
}

impl BasicAuth {
    fn apply(&self, header_map: &mut http::header::HeaderMap) {
        let auth = headers::Authorization::basic(&self.user, &self.password);
        header_map.typed_insert(auth)
    }
}

#[typetag::serde(name = "cratedb")]
impl SinkConfig for CrateDBSinkConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = cratedb(self.clone(), acker)?;

        let healtcheck = healthcheck(self.host.clone(), self.basic_auth.clone())?;
        Ok((sink, healtcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "cratedb"
    }
}

#[derive(Debug)]
enum BufferValue {
    Statement(String),
    BulkArgs(Vec<Vec<MapValue>>),
}

impl Serialize for BufferValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            BufferValue::Statement(s) => s.serialize(serializer),
            BufferValue::BulkArgs(ba) => ba.serialize(serializer),
        }
    }
}

pub struct CrateDBBuffer {
    statement: String,
    inner: Vec<Vec<MapValue>>,
}

impl CrateDBBuffer {
    pub fn new(statement: String) -> Self {
        Self {
            statement: statement.clone(),
            inner: Vec::new(),
        }
    }
}

impl Batch for CrateDBBuffer {
    type Input = Vec<MapValue>;
    type Output = Vec<u8>;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn push(&mut self, item: Self::Input) {
        self.inner.push(item)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            statement: self.statement.clone(),
            inner: Vec::new(),
        }
    }

    fn finish(self) -> Self::Output {
        let num_events = self.inner.len();
        let body = json!({
            "stmt": BufferValue::Statement(self.statement),
            "bulk_args": BufferValue::BulkArgs(self.inner),
        });
        info!("Sending {} events", num_events);
        let out = serde_json::to_vec(&body).unwrap_or(Vec::new());
        out
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

fn cratedb(config: CrateDBSinkConfig, acker: Acker) -> Result<super::RouterSink, String> {
    let host = build_uri(&config.host)?;
    let schema = config.schema.unwrap_or(String::from("doc"));
    let table = config.table.unwrap_or(String::from("logging"));
    let columns = config
        .columns
        .unwrap_or(vec![String::from("timestamp"), String::from("message")]);
    let keys = config
        .keys
        .unwrap_or(vec![String::from("timestamp"), String::from("message")]);
    let params = vec!["?"; columns.len()];
    let statement = format!(
        "INSERT INTO \"{}\".\"{}\" ({}) VALUES ({})",
        schema,
        table,
        columns.join(", "),
        params.join(", ")
    );

    let batch = config.batch.unwrap_or(1000, 1);

    let timeout = config.request_timeout_secs.unwrap_or(30);
    let in_flight_limit = config.request_in_flight_limit.unwrap_or(10);
    let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
    let rate_limit_num = config.request_rate_limit_num.unwrap_or(10);
    let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
    let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);
    let basic_auth = config.basic_auth.clone();

    let policy = FixedRetryPolicy::new(
        retry_attempts,
        Duration::from_secs(retry_backoff_secs),
        HttpRetryLogic,
    );

    let http_service = HttpService::new(move |body: Vec<u8>| {
        let mut builder = hyper::Request::builder();
        builder.method("POST");
        builder.uri(host.clone());

        let mut request = builder.body(body).unwrap();

        if let Some(auth) = &basic_auth {
            auth.apply(request.headers_mut());
        }

        request
    });

    let service = ServiceBuilder::new()
        .concurrency_limit(in_flight_limit)
        .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
        .retry(policy)
        .timeout(Duration::from_secs(timeout))
        .service(http_service);

    let sink = BatchServiceSink::new(service, acker)
        .batched_with_min(CrateDBBuffer::new(statement), &batch)
        .with_flat_map(move |event| stream::iter_ok(encode_event(event, &keys)));

    Ok(Box::new(sink))
}

fn healthcheck(host: String, auth: Option<BasicAuth>) -> crate::Result<super::Healthcheck> {
    let uri = build_uri(&host)?;
    let body = Body::from(String::from("{\"stmt\":\"SELECT name FROM sys.cluster\"}"));
    let mut request = Request::post(&uri).body(body).unwrap();

    if let Some(auth) = auth {
        auth.apply(request.headers_mut());
    }

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.into())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
            }
        });

    Ok(Box::new(healthcheck))
}

fn build_uri(raw: &str) -> Result<Uri, String> {
    let base: Uri = raw
        .parse()
        .map_err(|e| format!("invalid uri ({}): {:?}", e, raw))?;
    Ok(Uri::builder()
        .scheme(base.scheme_str().unwrap_or("http"))
        .authority(
            base.authority_part()
                .map(|a| a.as_str())
                .unwrap_or("127.0.0.1"),
        )
        .path_and_query("/_sql")
        .build()
        .expect("bug building uri"))
}

fn encode_event(event: Event, keys: &Vec<String>) -> Result<Vec<MapValue>, ()> {
    let event_map = event.into_log().unflatten();
    let mut values = Vec::new();
    for key in keys {
        let value = event_map
            .get(&Atom::from(key.as_str()))
            .unwrap_or(&MapValue::Null)
            .clone();
        values.push(value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        event::ValueKind,
        runtime::Runtime,
        sinks::cratedb::CrateDBSinkConfig,
        test_util::{next_addr, shutdown_on_idle},
        topology::config::SinkConfig,
    };
    use bytes::Buf;
    use chrono::{offset::TimeZone, Utc};
    use futures::{stream, sync::mpsc, Future, Sink, Stream};
    use headers::Authorization;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server};
    use std::io::{BufRead, BufReader};

    #[test]
    fn http_encode_event_json() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert_explicit("k1".into(), "v1".into());
        event.as_mut_log().insert_explicit("k2".into(), "v2".into());
        event.as_mut_log().insert_explicit("k3".into(), "v3".into());
        event.as_mut_log().insert_explicit("k4".into(), "v4".into());
        event.as_mut_log().insert_explicit("k5".into(), "v5".into());

        let unflattened_event =
            encode_event(event, &vec!["message".into(), "k1".into(), "k4".into()]).unwrap();
        assert_eq!(
            unflattened_event,
            vec![
                MapValue::Value("hello world".into()),
                MapValue::Value("v1".into()),
                MapValue::Value("v4".into())
            ]
        );
    }

    #[test]
    fn serialize_buffer_empty() {
        let input_buffer =
            CrateDBBuffer::new("INSERT INTO foo (a, b, c) VALUES (?, ?, ?)".to_string());
        assert_eq!(input_buffer.len(), 0);
        assert_eq!(input_buffer.is_empty(), true);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                std::str::from_utf8(&input_buffer.finish()).unwrap()
            )
            .unwrap(),
            json!({
                "stmt": "INSERT INTO foo (a, b, c) VALUES (?, ?, ?)",
                "bulk_args": []
            })
        );
    }

    #[test]
    fn serialize_buffer_single() {
        let mut input_buffer =
            CrateDBBuffer::new("INSERT INTO foo (a, b, c) VALUES (?, ?, ?)".to_string());
        input_buffer.push(vec![
            MapValue::Value(ValueKind::Bytes("val-a".into())),
            MapValue::Value(ValueKind::Integer(123)),
            MapValue::Value(ValueKind::Float(456.789)),
        ]);
        assert_eq!(input_buffer.len(), 1);
        assert_eq!(input_buffer.is_empty(), false);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                std::str::from_utf8(&input_buffer.finish()).unwrap()
            )
            .unwrap(),
            json!({
                "stmt": "INSERT INTO foo (a, b, c) VALUES (?, ?, ?)",
                "bulk_args": [["val-a", 123, 456.789]]
            })
        );
    }

    #[test]
    fn serialize_buffer_multiple() {
        let mut input_buffer =
            CrateDBBuffer::new("INSERT INTO foo (a, b, c) VALUES (?, ?, ?)".to_string());
        input_buffer.push(vec![
            MapValue::Value(ValueKind::Bytes("val-a".into())),
            MapValue::Value(ValueKind::Integer(123)),
            MapValue::Value(ValueKind::Float(456.789)),
        ]);
        input_buffer.push(vec![
            MapValue::Array(vec![
                MapValue::Value(ValueKind::Bytes("val-b1".into())),
                MapValue::Value(ValueKind::Bytes("val-b2".into())),
            ]),
            MapValue::Value(ValueKind::Boolean(true)),
            MapValue::Value(ValueKind::Timestamp(
                Utc.ymd(2019, 8, 9).and_hms_micro(12, 34, 56, 789000),
            )),
        ]);
        input_buffer.push(vec![
            MapValue::Value(ValueKind::Bytes("val-c".into())),
            MapValue::Value(ValueKind::Boolean(false)),
            MapValue::Null,
        ]);
        assert_eq!(input_buffer.len(), 3);
        assert_eq!(input_buffer.is_empty(), false);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                std::str::from_utf8(&input_buffer.finish()).unwrap()
            )
            .unwrap(),
            json!({
                "stmt": "INSERT INTO foo (a, b, c) VALUES (?, ?, ?)",
                "bulk_args": [
                    ["val-a", 123, 456.789],
                    [["val-b1", "val-b2"], true, "2019-08-09T12:34:56.789Z"],
                    ["val-c", false, null]
                ]
            }),
        );
    }

    #[test]
    fn sink_encoding() {
        let expected_body = json!({
                "stmt": "INSERT INTO \"schema1\".\"table1\" (ts, \"msg\", i, f, b, o, a) VALUES (?, ?, ?, ?, ?, ?, ?)",
                "bulk_args":[
                    [
                        "2019-08-09T12:34:56.000789Z",
                        "some very long text that \"may\" contain 'quotes'.",
                        123,
                        456.789,
                        true,
                        {"k1":"v1","k2":{"k21":"v21","k22":"v22"}},
                        [false,"bla",null,null,5]
                    ]
                ]
            }
        );
        let expected_response = r#""#;

        let in_addr = next_addr();

        let config = r#"
        host = "http://$IN_ADDR/"
        user = "jane"
        password = "doe"
        schema = "schema1"
        table = "table1"
        columns = ["ts", '"msg"', "i", "f", "b", "o", "a"]
        keys = ["time", "message", "int", "float", "bool", "obj", "array"]
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: CrateDBSinkConfig = toml::from_str(&config).unwrap();

        let (sink, _healthcheck) = config.build(Acker::Null).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr, expected_body, &expected_response);

        let mut events = vec![Event::new_empty_log()];
        events[0].as_mut_log().insert_explicit(
            "time".into(),
            ValueKind::Timestamp(Utc.ymd(2019, 8, 9).and_hms_micro(12, 34, 56, 789)),
        );
        events[0].as_mut_log().insert_explicit(
            "message".into(),
            "some very long text that \"may\" contain 'quotes'.".into(),
        );
        events[0]
            .as_mut_log()
            .insert_explicit("int".into(), 123.into());
        events[0]
            .as_mut_log()
            .insert_explicit("float".into(), 456.789.into());
        events[0]
            .as_mut_log()
            .insert_explicit("bool".into(), true.into());
        events[0]
            .as_mut_log()
            .insert_explicit("obj.k1".into(), "v1".into());
        events[0]
            .as_mut_log()
            .insert_explicit("obj.k2.k21".into(), "v21".into());
        events[0]
            .as_mut_log()
            .insert_explicit("obj.k2.k22".into(), "v22".into());
        events[0]
            .as_mut_log()
            .insert_explicit("array[0]".into(), false.into());
        events[0]
            .as_mut_log()
            .insert_explicit("array[1]".into(), "bla".into());
        events[0]
            .as_mut_log()
            .insert_explicit("array[4]".into(), 5.into());
        events[0]
            .as_mut_log()
            .insert_explicit("ignore".into(), "ignored key".into());
        let stream = stream::iter_ok(events.clone().into_iter());
        let pump = sink.send_all(stream);

        let mut rt = Runtime::new().unwrap();
        rt.spawn(server);

        let (sink, _) = rt.block_on(pump).unwrap();
        drop(sink);
        drop(trigger);

        let _output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!("/_sql", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("jane", "doe")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);
    }

    fn build_test_server(
        addr: &std::net::SocketAddr,
        expected_body: serde_json::Value,
        send_response: &'static str,
    ) -> (
        mpsc::Receiver<(http::request::Parts, hyper::Chunk)>,
        stream_cancel::Trigger,
        impl Future<Item = (), Error = ()> + Send + 'static,
    ) {
        let (_, rx) = mpsc::channel(100);
        let service = make_service_fn(move |_| {
            let expected_body = expected_body.clone();

            service_fn(move |req: Request<Body>| {
                let (_parts, body) = req.into_parts();
                let expected_body = expected_body.clone();

                body.concat2().and_then(move |body| {
                    let val = serde_json::from_slice::<serde_json::Value>(&body[..]).unwrap();
                    assert_eq!(expected_body, val);
                    Ok(Response::new(Body::from(send_response)))
                })
            })
        });

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        let server = Server::bind(addr)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|e| panic!("server error: {}", e));

        (rx, trigger, server)
    }
}

#[cfg(test)]
#[cfg(feature = "cratedb-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        event::ValueKind,
        test_util::{block_on, random_string},
        Event,
    };
    use chrono::{offset::TimeZone, Utc};
    use futures::Sink;
    use reqwest;
    use reqwest::header::CONTENT_TYPE;
    use serde_json::Value;

    #[test]
    fn insert_events() {
        crate::test_util::trace_init();
        let schema = random_string(10).to_lowercase();
        let table = random_string(10).to_lowercase();
        let host = String::from("http://localhost:4200");
        let config = CrateDBSinkConfig {
            host: host.clone(),
            schema: Some(schema.clone()),
            table: Some(table.clone()),
            columns: Some(vec![
                String::from("time"),
                String::from("msg"),
                String::from("bytes"),
            ]),
            keys: Some(vec![
                String::from("timestamp"),
                String::from("message"),
                String::from("size"),
            ]),
            ..Default::default()
        };

        let client = CrateDBClient::new(host, schema, table);
        client.post("SELECT name FROM sys.cluster".into());
        client.create_table(r#""time" TIMESTAMP, "msg" STRING, "bytes" INTEGER"#);

        let (sink, _hc) = config.build(Acker::Null).unwrap();

        let mut events = vec![Event::new_empty_log(), Event::new_empty_log()];
        events[0].as_mut_log().insert_explicit(
            "timestamp".into(),
            ValueKind::Timestamp(Utc.ymd(2019, 8, 9).and_hms_micro(12, 34, 56, 789)),
        );
        events[0]
            .as_mut_log()
            .insert_explicit("message".into(), "message 1".into());
        events[1].as_mut_log().insert_explicit(
            "timestamp".into(),
            ValueKind::Timestamp(Utc.ymd(2019, 8, 9).and_hms_micro(13, 34, 56, 789)),
        );
        events[1]
            .as_mut_log()
            .insert_explicit("message".into(), "message 2".into());
        events[1]
            .as_mut_log()
            .insert_explicit("size".into(), (42).into());

        let stream = stream::iter_ok(events.clone().into_iter());
        let pump = sink.send_all(stream);
        let (sink, _) = block_on(pump).unwrap();
        drop(sink);

        let output = client.select_all();
        assert_eq!(2, output.rowcount);

        let expected: Vec<Value> = vec![
            serde_json::from_str(r#"[1565354096000, "message 1", null]"#).unwrap(),
            serde_json::from_str(r#"[1565357696000, "message 2", 42]"#).unwrap(),
        ];

        assert_eq!(expected, output.rows);
    }

    struct CrateDBClient {
        host: String,
        table_ident: String,
        client: reqwest::Client,
    }

    impl CrateDBClient {
        fn new(host: String, schema: String, table: String) -> Self {
            CrateDBClient {
                host: host,
                table_ident: format!(r#""{}"."{}""#, schema, table),
                client: reqwest::Client::new(),
            }
        }

        fn post(&self, query: String) -> reqwest::Response {
            let body = json!({
                "stmt": query,
            });
            let body = serde_json::to_vec(&body).unwrap();
            let uri = format!("{}/_sql", &self.host.as_str());
            let mut response = self
                .client
                .post(uri.as_str())
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .unwrap();
            if !response.status().is_success() {
                panic!(
                    "Failed executing query {} on {}: {}",
                    String::from_utf8(body).unwrap(),
                    uri,
                    response.text().unwrap()
                )
            };
            return response;
        }

        fn create_table(&self, columns: &str) {
            self.post(format!(
                r#"DROP TABLE IF EXISTS {}"#,
                &self.table_ident.as_str(),
            ));
            self.post(format!(
                r#"CREATE TABLE {} ({})"#,
                &self.table_ident.as_str(),
                columns
            ));
        }

        fn select_all(&self) -> QueryResponse {
            self.post(format!(r#"REFRESH TABLE {}"#, &self.table_ident.as_str()));
            let mut response = self.post(format!(
                r#"SELECT "time", "msg", "bytes" FROM {} ORDER BY "time""#,
                &self.table_ident.as_str(),
            ));
            if let Ok(value) = response.json() {
                value
            } else {
                panic!("json failed: {:?}", response.text().unwrap());
            }
        }
    }

    #[derive(Debug, Deserialize)]
    struct QueryResponse {
        rows: Vec<Value>,
        rowcount: usize,
    }
}
