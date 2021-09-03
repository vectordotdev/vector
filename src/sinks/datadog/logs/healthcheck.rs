use super::ApiKey;
use crate::{
    http::HttpClient,
    sinks::util::{http::HttpSink, PartitionInnerBuffer},
};
use http::StatusCode;
use hyper::body::Body;
use std::sync::Arc;

/// The healthcheck is performed by sending an empty request to Datadog and
/// checking the return.
pub async fn healthcheck<T, O>(sink: T, client: HttpClient, api_key: String) -> crate::Result<()>
where
    T: HttpSink<Output = PartitionInnerBuffer<Vec<O>, ApiKey>>,
{
    let req = sink
        .build_request(PartitionInnerBuffer::new(
            Vec::with_capacity(0),
            Arc::from(api_key),
        ))
        .await?
        .map(Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

    match status {
        StatusCode::OK => Ok(()),
        StatusCode::UNAUTHORIZED => {
            let json: serde_json::Value = serde_json::from_slice(&body[..])?;

            Err(json
                .as_object()
                .and_then(|o| o.get("error"))
                .and_then(|s| s.as_str())
                .unwrap_or("Token is not valid, 401 returned.")
                .to_string()
                .into())
        }
        _ => {
            let body = String::from_utf8_lossy(&body[..]);

            Err(format!(
                "Server returned unexpected error status: {} body: {}",
                status, body
            )
            .into())
        }
    }
}
