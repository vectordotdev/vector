use super::util::{self, retries::FixedRetryPolicy, Buffer, Compression, ServiceSink, SinkExt};
use crate::record::Record;
use futures::{Future, Sink};
use http::Uri;
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tower_in_flight_limit::InFlightLimit;
use tower_retry::Retry;
use tower_timeout::Timeout;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: String,
    pub index: String,
    pub doc_type: String,
    pub id_key: Option<String>,
    pub buffer_size: Option<usize>,
    pub compression: Option<Compression>,
    pub request_timeout_secs: Option<u64>,
    pub retries: Option<usize>,
    pub in_flight_request_limit: Option<usize>,
}

#[typetag::serde(name = "elasticsearch")]
impl crate::topology::config::SinkConfig for ElasticSearchConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((es(self.clone()), healthcheck(self.host.clone())))
    }
}

fn es(config: ElasticSearchConfig) -> super::RouterSink {
    let host = config.host.clone();
    let id_key = config.id_key.clone();
    let buffer_size = config.buffer_size.unwrap_or(2 * 1024 * 1024);
    let gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let timeout_secs = config.request_timeout_secs.unwrap_or(10);
    let retries = config.retries.unwrap_or(5);
    let in_flight_limit = config.in_flight_request_limit.unwrap_or(1);

    let inner = util::http::HttpService::new();
    let timeout = Timeout::new(inner, Duration::from_secs(timeout_secs));
    let limited = InFlightLimit::new(timeout, in_flight_limit);

    let policy = FixedRetryPolicy::new(retries, Duration::from_secs(1), util::http::HttpRetryLogic);
    let service = Retry::new(policy, limited);

    let sink = ServiceSink::new(service)
        .with(move |body: Buffer| {
            let uri = format!("{}/_bulk", host);
            let uri: Uri = uri.parse().unwrap();

            let mut request = util::http::Request::post(uri, body.into());
            request
                .header("Content-Type", "application/x-ndjson")
                .header("Content-Encoding", "gzip");

            Ok(request)
        })
        .batched(Buffer::new(gzip), buffer_size)
        .with(move |record: Record| {
            let mut action = json!({
                "index": {
                    "_index": config.index,
                    "_type": config.doc_type,
                }
            });
            maybe_set_id(id_key.as_ref(), &mut action, &record);

            let mut body = serde_json::to_vec(&action).unwrap();
            body.push(b'\n');

            serde_json::to_writer(&mut body, &record).unwrap();
            body.push(b'\n');
            Ok(body)
        });

    Box::new(sink)
}

fn maybe_set_id(key: Option<impl AsRef<str>>, doc: &mut serde_json::Value, record: &Record) {
    let id = key.and_then(|k| record.custom.get(&k.as_ref().into()));
    if let Some(val) = id {
        doc.as_object_mut()
            .unwrap()
            .insert("_id".into(), json!(val));
    }
}

fn healthcheck(host: String) -> super::Healthcheck {
    let uri = format!("{}/_cluster/health", host);
    let request = Request::get(uri).body(Body::empty()).unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);
    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            if response.status() == hyper::StatusCode::OK {
                Ok(())
            } else {
                Err(format!("Unexpected status: {}", response.status()))
            }
        });

    Box::new(healthcheck)
}

#[cfg(test)]
mod tests {
    use super::maybe_set_id;
    use crate::Record;
    use serde_json::json;

    #[test]
    fn sets_id_from_custom_field() {
        let id_key = Some("foo");
        let mut record = Record::from("butts");
        record.custom.insert("foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &record);

        assert_eq!(json!({"_id": "bar"}), action);
    }

    #[test]
    fn doesnt_set_id_when_field_missing() {
        let id_key = Some("foo");
        let mut record = Record::from("butts");
        record.custom.insert("not_foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &record);

        assert_eq!(json!({}), action);
    }

    #[test]
    fn doesnt_set_id_when_not_configured() {
        let id_key: Option<&str> = None;
        let mut record = Record::from("butts");
        record.custom.insert("foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &record);

        assert_eq!(json!({}), action);
    }
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::ElasticSearchConfig;
    use crate::{
        test_util::{block_on, random_records_with_stream, random_string},
        topology::config::SinkConfig,
        Record,
    };
    use elastic::client::SyncClientBuilder;
    use futures::{Future, Sink};
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;
    use serde_json::{json, Value};

    #[test]
    fn insert_records() {
        let index = gen_index();
        let config = ElasticSearchConfig {
            host: "http://localhost:9200/".into(),
            index: index.clone(),
            doc_type: "log_lines".into(),
            id_key: None,
            buffer_size: None,
            compression: None,
            request_timeout_secs: None,
            retries: None,
            in_flight_request_limit: None,
        };

        let (sink, _hc) = config.build().unwrap();

        let (input, records) = random_records_with_stream(100, 100);

        let pump = sink.send_all(records);
        block_on(pump).unwrap();

        // make sure writes all all visible
        block_on(flush(config.host)).unwrap();

        let client = SyncClientBuilder::new().build().unwrap();

        let response = client
            .search::<Value>()
            .index(index)
            .body(json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .unwrap();

        assert_eq!(input.len() as u64, response.total());
        for hit in response.into_hits() {
            let record: Record = serde_json::from_value(hit.into_document().unwrap()).unwrap();
            assert!(input.contains(&record));
        }
    }

    fn gen_index() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }

    fn flush(host: String) -> impl Future<Item = (), Error = String> {
        let uri = format!("{}/_flush", host);
        let request = Request::post(uri).body(Body::empty()).unwrap();

        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client = Client::builder().build(https);
        client
            .request(request)
            .map_err(|err| err.to_string())
            .and_then(|response| {
                if response.status() == hyper::StatusCode::OK {
                    Ok(())
                } else {
                    Err(format!("Unexpected status: {}", response.status()))
                }
            })
    }

}
