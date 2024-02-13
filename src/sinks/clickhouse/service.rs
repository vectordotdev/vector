use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request, Response, StatusCode, Uri,
};
use hyper::{body, Body};
use snafu::ResultExt;
use std::task::{Context, Poll};
use tracing::Instrument;

use crate::{
    http::{Auth, HttpClient, HttpError},
    sinks::{
        clickhouse::config::Format,
        prelude::*,
        util::{http::HttpRetryLogic, retries::RetryAction},
        UriParseSnafu,
    },
};

#[derive(Debug, Clone)]
pub struct ClickhouseRequest {
    pub database: String,
    pub table: String,
    pub format: Format,
    pub body: Bytes,
    pub compression: Compression,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl MetaDescriptive for ClickhouseRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl Finalizable for ClickhouseRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

pub struct ClickhouseResponse {
    http_response: Response<Bytes>,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
}

impl DriverResponse for ClickhouseResponse {
    fn event_status(&self) -> EventStatus {
        match self.http_response.status().is_success() {
            true => EventStatus::Delivered,
            false => EventStatus::Rejected,
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ClickhouseRetryLogic {
    inner: HttpRetryLogic,
}

impl RetryLogic for ClickhouseRetryLogic {
    type Error = HttpError;
    type Response = ClickhouseResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        match response.http_response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.http_response.body();

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
            _ => self.inner.should_retry_response(&response.http_response),
        }
    }
}

/// `ClickhouseService` is a `Tower` service used to send logs to Clickhouse.
#[derive(Debug, Clone)]
pub struct ClickhouseService {
    client: HttpClient,
    auth: Option<Auth>,
    endpoint: Uri,
    skip_unknown_fields: bool,
    date_time_best_effort: bool,
}

impl ClickhouseService {
    /// Creates a new `ClickhouseService`.
    pub const fn new(
        client: HttpClient,
        auth: Option<Auth>,
        endpoint: Uri,
        skip_unknown_fields: bool,
        date_time_best_effort: bool,
    ) -> Self {
        Self {
            client,
            auth,
            endpoint,
            skip_unknown_fields,
            date_time_best_effort,
        }
    }
}

impl Service<ClickhouseRequest> for ClickhouseService {
    type Response = ClickhouseResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of Error internal event is handled upstream by the caller.
    fn call(&mut self, request: ClickhouseRequest) -> Self::Future {
        let mut client = self.client.clone();
        let auth = self.auth.clone();

        // Build the URI outside of the boxed future to avoid unnecessary clones.
        let uri = set_uri_query(
            &self.endpoint,
            &request.database,
            &request.table,
            request.format,
            self.skip_unknown_fields,
            self.date_time_best_effort,
        );

        Box::pin(async move {
            let mut builder = Request::post(&uri?)
                .header(CONTENT_TYPE, "application/x-ndjson")
                .header(CONTENT_LENGTH, request.body.len());
            if let Some(ce) = request.compression.content_encoding() {
                builder = builder.header(CONTENT_ENCODING, ce);
            }
            if let Some(auth) = auth {
                builder = auth.apply_builder(builder);
            }

            let http_request = builder
                .body(Body::from(request.body))
                .expect("building HTTP request failed unexpectedly");

            let response = client.call(http_request).in_current_span().await?;
            let (parts, body) = response.into_parts();
            let body = body::to_bytes(body).await?;
            Ok(ClickhouseResponse {
                http_response: hyper::Response::from_parts(parts, body),
                raw_byte_size: request.metadata.request_encoded_size(),
                events_byte_size: request
                    .metadata
                    .into_events_estimated_json_encoded_byte_size(),
            })
        })
    }
}

fn set_uri_query(
    uri: &Uri,
    database: &str,
    table: &str,
    format: Format,
    skip_unknown: bool,
    date_time_best_effort: bool,
) -> crate::Result<Uri> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "query",
            format!(
                "INSERT INTO \"{}\".\"{}\" FORMAT {}",
                database,
                table.replace('\"', "\\\""),
                format
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_valid() {
        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            Format::JsonEachRow,
            false,
            true,
        )
        .unwrap();
        assert_eq!(uri.to_string(), "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     date_time_input_format=best_effort&\
                                     query=INSERT+INTO+%22my_database%22.%22my_table%22+FORMAT+JSONEachRow");

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonEachRow,
            false,
            false,
        )
        .unwrap();
        assert_eq!(uri.to_string(), "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     query=INSERT+INTO+%22my_database%22.%22my_%5C%22table%5C%22%22+FORMAT+JSONEachRow");

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonAsObject,
            true,
            true,
        )
        .unwrap();
        assert_eq!(uri.to_string(), "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     input_format_skip_unknown_fields=1&\
                                     date_time_input_format=best_effort&\
                                     query=INSERT+INTO+%22my_database%22.%22my_%5C%22table%5C%22%22+FORMAT+JSONAsObject");
    }

    #[test]
    fn encode_invalid() {
        set_uri_query(
            &"localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            Format::JsonEachRow,
            false,
            false,
        )
        .unwrap_err();
    }
}
