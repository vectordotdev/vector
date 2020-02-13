use crate::dns::Resolver;
use futures::{
    future::{Future, FutureResult},
    Poll,
};
use hyper::client::connect::HttpConnector;
use hyper_openssl::{
    openssl::ssl::{SslConnector, SslMethod},
    HttpsConnector,
};
use rusoto_core::{CredentialsError, HttpClient, Region};
use rusoto_credential::{
    AutoRefreshingProvider, AutoRefreshingProviderFuture, AwsCredentials,
    DefaultCredentialsProvider, DefaultCredentialsProviderFuture, ProvideAwsCredentials,
    StaticProvider,
};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use snafu::{ResultExt, Snafu};

pub type Client = HttpClient<HttpsConnector<HttpConnector<Resolver>>>;

#[derive(Debug, Snafu)]
enum RusotoError {
    #[snafu(display("Invalid AWS credentials: {}", source))]
    InvalidAWSCredentials { source: CredentialsError },
}

// A place-holder for the types of AWS credentials we support
pub enum AwsCredentialsProvider {
    Default(DefaultCredentialsProvider),
    Role(AutoRefreshingProvider<StsAssumeRoleSessionCredentialsProvider>),
    Static(StaticProvider),
}

impl AwsCredentialsProvider {
    pub fn new(region: &Region, assume_role: Option<String>) -> crate::Result<Self> {
        if let Some(role) = assume_role {
            let sts = StsClient::new(region.clone());

            let provider = StsAssumeRoleSessionCredentialsProvider::new(
                sts,
                role,
                "default".to_owned(),
                None,
                None,
                None,
                None,
            );

            let creds = AutoRefreshingProvider::new(provider).context(InvalidAWSCredentials)?;
            Ok(Self::Role(creds))
        } else {
            let creds = DefaultCredentialsProvider::new().context(InvalidAWSCredentials)?;
            Ok(Self::Default(creds))
        }
    }

    pub fn new_minimal<A: Into<String>, S: Into<String>>(access_key: A, secret_key: S) -> Self {
        Self::Static(StaticProvider::new_minimal(
            access_key.into(),
            secret_key.into(),
        ))
    }
}

impl ProvideAwsCredentials for AwsCredentialsProvider {
    type Future = AwsCredentialsProviderFuture;
    fn credentials(&self) -> Self::Future {
        match self {
            Self::Default(p) => AwsCredentialsProviderFuture::Default(p.credentials()),
            Self::Role(p) => AwsCredentialsProviderFuture::Role(p.credentials()),
            Self::Static(p) => AwsCredentialsProviderFuture::Static(p.credentials()),
        }
    }
}

pub enum AwsCredentialsProviderFuture {
    Default(DefaultCredentialsProviderFuture),
    Role(AutoRefreshingProviderFuture<StsAssumeRoleSessionCredentialsProvider>),
    Static(FutureResult<AwsCredentials, CredentialsError>),
}

impl Future for AwsCredentialsProviderFuture {
    type Item = AwsCredentials;
    type Error = CredentialsError;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            Self::Default(f) => f.poll(),
            Self::Role(f) => f.poll(),
            Self::Static(f) => f.poll(),
        }
    }
}

pub fn client(resolver: Resolver) -> crate::Result<Client> {
    let mut http = HttpConnector::new_with_resolver(resolver);
    http.enforce_http(false);

    let ssl = SslConnector::builder(SslMethod::tls())?;
    let https = HttpsConnector::with_connector(http, ssl)?;

    Ok(HttpClient::from_connector(https))
}
