use crate::dns::Resolver;
use hyper::client::connect::HttpConnector;
use hyper_openssl::{
    openssl::ssl::{SslConnector, SslMethod},
    HttpsConnector,
};
use rusoto_core::HttpClient;

pub type Client = HttpClient<HttpsConnector<HttpConnector<Resolver>>>;

pub fn client(resolver: Resolver) -> crate::Result<Client> {
    let mut http = HttpConnector::new_with_resolver(resolver);
    http.enforce_http(false);

    let ssl = SslConnector::builder(SslMethod::tls())?;
    let https = HttpsConnector::with_connector(http, ssl)?;

    Ok(HttpClient::from_connector(https))
}
