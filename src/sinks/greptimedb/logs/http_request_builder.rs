use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    http::{Auth, HttpClient, HttpError},
    sinks::{
        prelude::*,
        util::http::{HttpRequest, HttpResponse, HttpRetryLogic, HttpServiceRequestBuilder},
        HTTPRequestBuilderSnafu, HealthcheckError,
    },
    Error,
};
use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request, StatusCode,
};
use hyper::Body;
use snafu::ResultExt;
use std::collections::HashMap;
use vector_lib::codecs::encoding::Framer;

/// Partition key for GreptimeDB logs sink.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(super) struct PartitionKey {
    pub dbname: String,
    pub table: String,
    pub pipeline_name: String,
    pub pipeline_version: Option<String>,
}

/// KeyPartitioner that partitions events by (dbname, table, pipeline_name, pipeline_version) pair.
pub(super) struct KeyPartitioner {
    dbname: Template,
    table: Template,
    pipeline_name: Template,
    pipeline_version: Option<Template>,
}

impl KeyPartitioner {
    pub const fn new(
        db: Template,
        table: Template,
        pipeline_name: Template,
        pipeline_version: Option<Template>,
    ) -> Self {
        Self {
            dbname: db,
            table,
            pipeline_name,
            pipeline_version,
        }
    }

    fn render(template: &Template, item: &Event, field: &'static str) -> Option<String> {
        template
            .render_string(item)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some(field),
                    drop_event: true,
                });
            })
            .ok()
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let dbname = Self::render(&self.dbname, item, "dbname_key")?;
        let table = Self::render(&self.table, item, "table_key")?;
        let pipeline_name = Self::render(&self.pipeline_name, item, "pipeline_name")?;
        let pipeline_version = self
            .pipeline_version
            .as_ref()
            .and_then(|template| Self::render(template, item, "pipeline_version"));
        Some(PartitionKey {
            dbname,
            table,
            pipeline_name,
            pipeline_version,
        })
    }
}

/// GreptimeDB logs HTTP request builder.
#[derive(Debug, Clone)]
pub(super) struct GreptimeDBLogsHttpRequestBuilder {
    pub(super) endpoint: String,
    pub(super) auth: Option<Auth>,
    pub(super) encoder: (Transformer, Encoder<Framer>),
    pub(super) compression: Compression,
    pub(super) extra_params: Option<HashMap<String, String>>,
}

fn prepare_log_ingester_url(
    endpoint: &str,
    db: &str,
    table: &str,
    metadata: &PartitionKey,
    extra_params: &Option<HashMap<String, String>>,
) -> String {
    let path = format!("{}/v1/events/logs", endpoint);
    let mut url = url::Url::parse(&path).unwrap();
    let mut url_builder = url.query_pairs_mut();
    url_builder
        .append_pair("db", db)
        .append_pair("table", table)
        .append_pair("pipeline_name", &metadata.pipeline_name);

    if let Some(pipeline_version) = metadata.pipeline_version.as_ref() {
        url_builder.append_pair("pipeline_version", pipeline_version);
    }

    if let Some(extra_params) = extra_params.as_ref() {
        for (key, value) in extra_params.iter() {
            url_builder.append_pair(key, value);
        }
    }

    url_builder.finish().to_string()
}

impl HttpServiceRequestBuilder<PartitionKey> for GreptimeDBLogsHttpRequestBuilder {
    fn build(&self, mut request: HttpRequest<PartitionKey>) -> Result<Request<Bytes>, Error> {
        let metadata = request.get_additional_metadata();
        let table = metadata.table.clone();
        let db = metadata.dbname.clone();

        // prepare url
        let url = prepare_log_ingester_url(
            self.endpoint.as_str(),
            &db,
            &table,
            metadata,
            &self.extra_params,
        );

        // prepare body
        let payload = request.take_payload();

        let mut builder = Request::post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header(CONTENT_LENGTH, payload.len());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header(CONTENT_ENCODING, ce);
        }

        if let Some(auth) = self.auth.clone() {
            builder = auth.apply_builder(builder);
        }

        builder
            .body(payload)
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for GreptimeDBLogsHttpRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = metadata;
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            PartitionKey {
                dbname: key.dbname,
                table: key.table,
                pipeline_name: key.pipeline_name,
                pipeline_version: key.pipeline_version,
            },
        )
    }
}

pub(super) async fn http_healthcheck(
    client: HttpClient,
    endpoint: String,
    auth: Option<Auth>,
) -> crate::Result<()> {
    let uri = format!("{endpoint}/health");
    let mut request = Request::get(uri).body(Body::empty())?;

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

/// GreptimeDB HTTP retry logic.
#[derive(Clone, Default)]
pub(super) struct GreptimeDBHttpRetryLogic {
    inner: HttpRetryLogic,
}

impl RetryLogic for GreptimeDBHttpRetryLogic {
    type Error = HttpError;
    type Response = HttpResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        self.inner.should_retry_response(&response.http_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_url() {
        let endpoint = "http://localhost:8080";
        let db = "test_db";
        let table = "test_table";
        let metadata = PartitionKey {
            dbname: "test_db".to_string(),
            table: "test_table".to_string(),
            pipeline_name: "test_pipeline".to_string(),
            pipeline_version: Some("test_version".to_string()),
        };
        let params = vec![("param1", "value1"), ("param2", "value2")];
        let extra_params = Some(
            params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );

        let url = prepare_log_ingester_url(endpoint, db, table, &metadata, &extra_params);
        let url = url::Url::parse(&url).unwrap();
        let check = url.query_pairs().all(|(k, v)| match k.as_ref() {
            "db" => v == "test_db",
            "table" => v == "test_table",
            "pipeline_name" => v == "test_pipeline",
            "pipeline_version" => v == "test_version",
            "param1" => v == "value1",
            "param2" => v == "value2",
            _ => false,
        });
        assert!(check);
    }
}
