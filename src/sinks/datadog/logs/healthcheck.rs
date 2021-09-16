use crate::http::HttpClient;
use http::{Request, Response, StatusCode, Uri};
use hyper::body::Body;

/// The healthcheck is performed by sending an empty request to Datadog and
/// checking the return.
pub async fn healthcheck(client: HttpClient, uri: Uri, api_key: String) -> crate::Result<()> {
    let body = vec![];
    let request: Request<Body> = Request::post(uri)
        .header("Content-Type", "application/json")
        .header("DD-API-KEY", &api_key[..])
        .header("Content-Length", body.len())
        .body(Body::from(body))?;
    let response: Response<Body> = client.send(request).await?;

    let status = response.status();
    let body = hyper::body::to_bytes(response.into_body()).await?;

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
