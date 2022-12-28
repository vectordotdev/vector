use crate::http::Auth;
use crate::http::HttpClient;
use crate::sinks::databend::api::DatabendHttpRequest;
use crate::sinks::util::UriSerde;

use super::api::http_query;

pub(crate) async fn select_one(
    client: HttpClient,
    endpoint: UriSerde,
    auth: Option<Auth>,
) -> crate::Result<()> {
    let req = DatabendHttpRequest::new("SELECT 1".to_string());
    let _ = http_query(client, endpoint, auth, req).await?;
    Ok(())
}
