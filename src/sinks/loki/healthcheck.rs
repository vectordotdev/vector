use super::append_loki_path;

use crate::{
    http::{Auth, HttpClient},
    sinks::util::{HttpEndpoint, UriSerde},
};

async fn fetch_status(
    endpoint: &http::Uri,
    auth: Option<&Auth>,
    client: &HttpClient,
) -> crate::Result<http::StatusCode> {
    let mut req = http::Request::get(endpoint)
        .body(hyper::Body::empty())
        .expect("Building request never fails.");

    if let Some(auth) = auth {
        auth.apply(&mut req);
    }

    Ok(client.send(req).await?.status())
}

pub async fn healthcheck(
    base_endpoint: HttpEndpoint,
    auth: Option<Auth>,
    healthcheck_uri: Option<UriSerde>,
    client: HttpClient,
) -> crate::Result<()> {
    // Healthcheck URI has been explicitly configured
    if let Some(uri) = healthcheck_uri {
        let auth = uri.auth.or(auth);
        let status = fetch_status(&uri.uri, auth.as_ref(), &client).await?;
        return match status {
            http::StatusCode::OK => Ok(()),
            _ => Err(format!("A non-successful status returned: {status}").into()),
        };
    }

    let status = match fetch_status(
        append_loki_path(&base_endpoint, "ready")?.as_uri(),
        auth.as_ref(),
        &client,
    )
    .await?
    {
        // Issue https://github.com/vectordotdev/vector/issues/6463
        http::StatusCode::NOT_FOUND => {
            debug!("Endpoint `/ready` not found. Retrying healthcheck with top level query.");
            // Probe the normalized base path with one trailing slash (`/loki/`,
            // not `/loki`), matching the pre-`HttpEndpoint` behavior. Reverse
            // proxies commonly redirect `/loki` to `/loki/`, and the healthcheck
            // rejects non-200 responses rather than following them.
            fetch_status(
                append_loki_path(&base_endpoint, "/")?.as_uri(),
                auth.as_ref(),
                &client,
            )
            .await?
        }
        status => status,
    };

    match status {
        http::StatusCode::OK => Ok(()),
        _ => Err(format!("A non-successful status returned: {status}").into()),
    }
}
