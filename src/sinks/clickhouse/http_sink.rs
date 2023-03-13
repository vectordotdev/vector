use bytes::{BufMut, Bytes, BytesMut};
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::ResultExt;

use super::ClickhouseConfig;
use crate::{
    codecs::Transformer,
    config::SinkContext,
    event::Event,
    http::{HttpClient, HttpError, MaybeAuth},
    sinks::{
        util::{
            http::{BatchedHttpSink, HttpEventEncoder, HttpRetryLogic, HttpSink},
            retries::{RetryAction, RetryLogic},
            Buffer, TowerRequestConfig,
        },
        Healthcheck, HealthcheckError, UriParseSnafu, VectorSink,
    },
    tls::TlsSettings,
};

pub(crate) async fn build_http_sink(
    cfg: &ClickhouseConfig,
    cx: SinkContext,
) -> crate::Result<(VectorSink, Healthcheck)> {
    let batch = cfg.batch.into_batch_settings()?;
    let request = cfg.request.unwrap_with(&TowerRequestConfig::default());
    let tls_settings = TlsSettings::from_options(&cfg.tls)?;
    let client = HttpClient::new(tls_settings, &cx.proxy)?;

    let config = ClickhouseConfig {
        auth: cfg.auth.choose_one(&cfg.endpoint.auth)?,
        ..cfg.clone()
    };

    let sink = BatchedHttpSink::with_logic(
        config.clone(),
        Buffer::new(batch.size, cfg.compression),
        ClickhouseRetryLogic::default(),
        request,
        batch.timeout,
        client.clone(),
    )
    .sink_map_err(|error| error!(message = "Fatal clickhouse sink error.", %error));

    let healthcheck = healthcheck(client, config).boxed();

    Ok((VectorSink::from_event_sink(sink), healthcheck))
}

pub struct ClickhouseEventEncoder {
    transformer: Transformer,
}

impl HttpEventEncoder<BytesMut> for ClickhouseEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<BytesMut> {
        self.transformer.transform(&mut event);
        let log = event.into_log();

        let mut body = crate::serde::json::to_bytes(&log).expect("Events should be valid json!");
        body.put_u8(b'\n');

        Some(body)
    }
}

#[async_trait::async_trait]
impl HttpSink for ClickhouseConfig {
    type Input = BytesMut;
    type Output = BytesMut;
    type Encoder = ClickhouseEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        ClickhouseEventEncoder {
            transformer: self.encoding.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Bytes>> {
        let database = if let Some(database) = &self.database {
            database.as_str()
        } else {
            "default"
        };

        let uri = set_uri_query(
            &self.endpoint.with_default_parts().uri,
            database,
            &self.table,
            self.skip_unknown_fields,
            self.date_time_best_effort,
        )
        .expect("Unable to encode uri");

        let mut builder = Request::post(&uri).header("Content-Type", "application/x-ndjson");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        let mut request = builder.body(events.freeze()).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        Ok(request)
    }
}

async fn healthcheck(client: HttpClient, config: ClickhouseConfig) -> crate::Result<()> {
    // TODO: check if table exists?
    let uri = format!("{}/?query=SELECT%201", config.endpoint.with_default_parts());
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = &config.auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

fn set_uri_query(
    uri: &Uri,
    database: &str,
    table: &str,
    skip_unknown: bool,
    date_time_best_effort: bool,
) -> crate::Result<Uri> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "query",
            format!(
                "INSERT INTO \"{}\".\"{}\" FORMAT JSONEachRow",
                database,
                table.replace('\"', "\\\"")
            )
            .as_str(),
        )
        .finish();

    let mut uri = uri.to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }
    uri.push_str("?input_format_import_nested_json=1&");
    if skip_unknown {
        uri.push_str("input_format_skip_unknown_fields=1&");
    }
    if date_time_best_effort {
        uri.push_str("date_time_input_format=best_effort&")
    }
    uri.push_str(query.as_str());

    uri.parse::<Uri>()
        .context(UriParseSnafu)
        .map_err(Into::into)
}

#[derive(Debug, Default, Clone)]
struct ClickhouseRetryLogic {
    inner: HttpRetryLogic,
}

impl RetryLogic for ClickhouseRetryLogic {
    type Error = HttpError;
    type Response = http::Response<Bytes>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        match response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.body();

                // Currently, ClickHouse returns 500's incorrect data and type mismatch errors.
                // This attempts to check if the body starts with `Code: {code_num}` and to not
                // retry those errors.
                //
                // Reference: https://github.com/vectordotdev/vector/pull/693#issuecomment-517332654
                // Error code definitions: https://github.com/ClickHouse/ClickHouse/blob/master/dbms/src/Common/ErrorCodes.cpp
                //
                // Fix already merged: https://github.com/ClickHouse/ClickHouse/pull/6271
                if body.starts_with(b"Code: 117") {
                    RetryAction::DontRetry("incorrect data".into())
                } else if body.starts_with(b"Code: 53") {
                    RetryAction::DontRetry("type mismatch".into())
                } else {
                    RetryAction::Retry(String::from_utf8_lossy(body).to_string().into())
                }
            }
            _ => self.inner.should_retry_response(response),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ClickhouseConfig>();
    }

    #[test]
    fn encode_valid() {
        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            false,
            true,
        )
        .unwrap();
        assert_eq!(uri.to_string(), "http://localhost:80/?input_format_import_nested_json=1&date_time_input_format=best_effort&query=INSERT+INTO+%22my_database%22.%22my_table%22+FORMAT+JSONEachRow");

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            false,
            false,
        )
        .unwrap();
        assert_eq!(uri.to_string(), "http://localhost:80/?input_format_import_nested_json=1&query=INSERT+INTO+%22my_database%22.%22my_%5C%22table%5C%22%22+FORMAT+JSONEachRow");
    }

    #[test]
    fn encode_invalid() {
        set_uri_query(
            &"localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            false,
            false,
        )
        .unwrap_err();
    }
}
