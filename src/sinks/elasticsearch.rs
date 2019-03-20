use super::util::{self, Buffer, SinkExt};
use crate::record::Record;
use futures::{Future, Sink};
use http::Uri;
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: String,
    pub index: String,
    pub doc_type: String,
}

#[typetag::serde(name = "elasticsearch")]
impl crate::topology::config::SinkConfig for ElasticSearchConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((es(self.clone()), healthcheck(self.host.clone())))
    }
}

fn es(config: ElasticSearchConfig) -> super::RouterSink {
    let host = config.host.clone();
    let sink = util::http::HttpSink::new()
        .with(move |body: Buffer| {
            let uri = format!("{}/_bulk", host);
            let uri: Uri = uri.parse().unwrap();

            let mut request = util::http::Request::post(uri, body.into());
            request
                .header("Content-Type", "application/x-ndjson")
                .header("Content-Encoding", "gzip");

            Ok(request)
        })
        .batched(Buffer::new(true), 2 * 1024 * 1024)
        .with(move |record: Record| {
            let action = json!({
                "index": {
                    "_index": config.index,
                    "_type": config.doc_type,
                }
            });
            let mut body = serde_json::to_vec(&action).unwrap();
            body.push(b'\n');

            serde_json::to_writer(&mut body, &record).unwrap();
            body.push(b'\n');
            Ok(body)
        });

    Box::new(sink)
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
#[cfg(feature = "es-integration-tests")]
mod tests {
    use super::ElasticSearchConfig;
    use crate::{
        test_util::{block_on, random_lines},
        topology::config::SinkConfig,
        Record,
    };
    use elastic::client::SyncClientBuilder;
    use futures::{stream, Future, Sink};
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
        };

        let (sink, _hc) = config.build().unwrap();

        let input = random_lines(100)
            .map(Record::from)
            .take(100)
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(input.clone().into_iter()));
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
        format!("test-{}", random_lines(10).next().unwrap().to_lowercase())
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
