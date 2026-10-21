use http_1::{Request, StatusCode};

use super::append_loki_path;

use crate::{
    http::{
        Auth,
        client_v1::{HttpClient, empty_body},
    },
    sinks::util::{HttpEndpoint, UriSerde},
};

async fn fetch_status(
    uri: http_1::Uri,
    auth: Option<&Auth>,
    client: &HttpClient,
) -> crate::Result<StatusCode> {
    let mut request = Request::get(uri).body(empty_body())?;

    if let Some(auth) = auth {
        auth.apply_v1(&mut request);
    }

    let response = client.send(request).await?;

    Ok(response.status())
}

pub async fn healthcheck(
    base_endpoint: HttpEndpoint,
    auth: Option<Auth>,
    healthcheck_uri: Option<UriSerde>,
    client: HttpClient,
) -> crate::Result<()> {
    // Healthcheck URI has been explicitly configured
    if let Some(uri) = healthcheck_uri {
        let status =
            fetch_status(uri.uri_v1()?, uri.auth.as_ref().or(auth.as_ref()), &client).await?;
        return match status {
            StatusCode::OK => Ok(()),
            _ => Err(format!("A non-successful status returned: {status}").into()),
        };
    }

    let status = match fetch_status(
        append_loki_path(&base_endpoint, "ready")?.into_v1(),
        auth.as_ref(),
        &client,
    )
    .await?
    {
        // Issue https://github.com/vectordotdev/vector/issues/6463
        StatusCode::NOT_FOUND => {
            debug!("Endpoint `/ready` not found. Retrying healthcheck with top level query.");
            // Probe the normalized base path with one trailing slash (`/loki/`,
            // not `/loki`), matching the pre-`HttpEndpoint` behavior. Reverse
            // proxies commonly redirect `/loki` to `/loki/`, and the healthcheck
            // rejects non-200 responses rather than following them.
            fetch_status(
                append_loki_path(&base_endpoint, "/")?.into_v1(),
                auth.as_ref(),
                &client,
            )
            .await?
        }
        status => status,
    };

    match status {
        StatusCode::OK => Ok(()),
        _ => Err(format!("A non-successful status returned: {status}").into()),
    }
}
