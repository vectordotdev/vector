use std::{
    sync::Arc,
    task::{Context, Poll},
};

use aws_sdk_cloudwatch::Region;
use bytes::Bytes;
use http::StatusCode;

use super::request_builder::RemoteWriteRequest;
use crate::{http::HttpClient, sinks::prelude::*};

#[derive(Clone)]
pub(super) struct RemoteWriteService {
    pub endpoint: Uri,
    pub aws_region: Option<Region>,
    pub credentials_provider: Option<SharedCredentialsProvider>,

    default_namespace: Option<String>,
    client: HttpClient,
    buckets: Vec<f64>,
    quantiles: Vec<f64>,
    compression: Compression,
}

impl RemoteWriteService {
    // fn encode_events(&self, metrics: Vec<Metric>) -> Bytes {
    //     let mut time_series = collector::TimeSeries::new();
    //     for metric in metrics {
    //         time_series.encode_metric(
    //             self.default_namespace.as_deref(),
    //             &self.buckets,
    //             &self.quantiles,
    //             &metric,
    //         );
    //     }
    //     let request = time_series.finish();

    //     let mut out = BytesMut::with_capacity(request.encoded_len());
    //     request.encode(&mut out).expect("Out of memory");
    //     out.freeze()
    // }
}

struct RemoteWriteResponse {
    byte_size: usize,
    json_size: GroupedCountByteSize,
    response: http::Response<Bytes>,
}

impl DriverResponse for RemoteWriteResponse {
    fn event_status(&self) -> EventStatus {
        if self.response.status().is_success() {
            EventStatus::Delivered
        } else {
            EventStatus::Errored
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.json_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

impl Service<RemoteWriteRequest> for RemoteWriteService {
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _task: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: RemoteWriteRequest) -> Self::Future {
        // let (events, key) = buffer.into_parts();
        // let body = self.encode_events(events);
        // let body = compress_block(self.compression, body);

        let client = self.client.clone();
        // let request_builder = Arc::clone(&self.http_request_builder);

        Box::pin(async move {
            // let request = request_builder
            //     .build_request(http::Method::POST, body, key.tenant_id)
            //     .await?;

            let metadata = std::mem::take(request.metadata_mut());
            let mut request = request.request;

            if let Some(credentials_provider) = &self.credentials_provider {
                sign_request(&mut request, credentials_provider, &self.aws_region).await?;
            }

            let (parts, body) = request.into_parts();
            let request: Request<hyper::Body> = hyper::Request::from_parts(parts, body.into());

            let (protocol, endpoint) = uri::protocol_endpoint(request.uri().clone());

            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            let byte_size = body.len();

            let response = hyper::Response::from_parts(parts, body);

            Ok(RemoteWriteResponse {
                byte_size,
                json_size: todo!(),
                response,
            })
        })
    }
}

async fn sign_request(
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    crate::aws::sign_request("aps", request, credentials_provider, region).await
}
