use crate::codecs::{Encoder, Transformer};
use crate::event::{Event, EventFinalizers, Finalizable};
use crate::http::{Auth, HttpClient, HttpError};
use crate::sinks::prelude::{
    Compression, EncodeResult, Partitioner, RequestBuilder, RequestMetadata,
    RequestMetadataBuilder, RetryAction, RetryLogic,
};
use crate::sinks::{HTTPRequestBuilderSnafu, HealthcheckError};
use crate::Error;
use bytes::Bytes;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::{Request, StatusCode};
use hyper::Body;
use snafu::ResultExt;

use vector_lib::codecs::encoding::Framer;

use crate::sinks::util::http::{HttpRequest, HttpResponse, HttpServiceRequestBuilder};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(super) struct PartitionKey {
    pub db: String,
    pub table: String,
}

impl Partitioner for PartitionKey {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, _item: &Self::Item) -> Self::Key {
        Some(PartitionKey {
            db: self.db.clone(),
            table: self.table.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct GreptimeDBLogsHttpRequestBuilder {
    pub(super) endpoint: String,
    pub(super) auth: Option<Auth>,
    pub(super) encoder: (Transformer, Encoder<Framer>),
}

impl HttpServiceRequestBuilder<PartitionKey> for GreptimeDBLogsHttpRequestBuilder {
    fn build(&self, mut request: HttpRequest<PartitionKey>) -> Result<Request<Bytes>, Error> {
        let metadata = request.get_additional_metadata();
        let table = metadata.table.clone();
        let db = metadata.db.clone();

        // prepare url
        let endpoint = format!("{}/v1/sql", self.endpoint.as_str());
        let mut url = url::Url::parse(&endpoint).unwrap();
        let url = url
            .query_pairs_mut()
            .append_pair("db", &db)
            .finish()
            .to_string();

        // prepare body
        let payload = request.take_payload();
        let message = String::from_utf8_lossy(payload.as_ref());
        let now = chrono::Local::now().timestamp_millis();
        let sql = format!("INSERT INTO {table}(time_local, message) values({now}, '{message}');");
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("sql", &sql)
            .finish();

        let mut builder = Request::post(&url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(CONTENT_LENGTH, body.len());

        // todo compression

        if let Some(auth) = self.auth.clone() {
            builder = auth.apply_builder(builder);
        }

        builder
            .body(body.into())
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
        Compression::None
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
                db: key.db,
                table: key.table,
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
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[derive(Clone, Default)]
pub(super) struct GreptimeDBHttpRetryLogic;

impl RetryLogic for GreptimeDBHttpRetryLogic {
    type Error = HttpError;
    type Response = HttpResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.http_response.status();
        match status {
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
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!(
                    "{}: {}",
                    status,
                    String::from_utf8_lossy(response.http_response.body())
                )
                .into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}
