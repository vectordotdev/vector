use http::Uri;

use super::config::LokiConfig;
use crate::{
    http::{Auth, HttpClient},
    sinks::util::UriSerde,
};

async fn fetch_status(
    endpoint: &Uri,
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
    config: LokiConfig,
    healthcheck_uri: Option<UriSerde>,
    client: HttpClient,
) -> crate::Result<()> {
    // Healthcheck URI has been explicitly configured
    if let Some(uri) = healthcheck_uri {
        let auth = uri.auth.or(config.auth);
        let status = fetch_status(&uri.uri, auth.as_ref(), &client).await?;
        return match status {
            http::StatusCode::OK => Ok(()),
            code => Err(format!("A non-successful status returned: {}", code.as_u16()).into()),
        };
    }

    let endpoint = config.endpoint.append_path("ready")?;
    let status = match fetch_status(&endpoint.uri, config.auth.as_ref(), &client).await? {
        // Issue https://github.com/vectordotdev/vector/issues/6463
        http::StatusCode::NOT_FOUND => {
            debug!("Endpoint `/ready` not found. Retrying healthcheck with top level query.");
            fetch_status(&config.endpoint.uri, config.auth.as_ref(), &client).await?
        }
        status => status,
    };

    match status {
        http::StatusCode::OK => Ok(()),
        _ => Err(format!("A non-successful status returned: {status}").into()),
    }
}
