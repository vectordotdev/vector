use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::datadog::logs::log_api::LogApi;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::Compression;
use http::{Request, Uri};
use hyper::Body;
use snafu::Snafu;
use std::sync::Arc;
use tokio::time::Duration;
use tower::Service;
use vector_core::config::{log_schema, LogSchema};

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("The builder is missing the URI to use for Datadog's logs API"))]
    MissingUri,
    #[snafu(display("The builder is missing an HTTP client to use Datadog's logs API"))]
    MissingHttpClient,
}

#[derive(Debug)]
pub struct LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    encoding: EncodingConfigWithDefault<Encoding>,
    http_client: Option<Client>,
    datadog_uri: Option<Uri>,
    default_api_key: Option<Arc<str>>,
    compression: Option<Compression>,
    timeout: Option<Duration>,
    bytes_stored_limit: u64,
    log_schema_message_key: Option<&'static str>,
    log_schema_timestamp_key: Option<&'static str>,
    log_schema_host_key: Option<&'static str>,
}

impl<Client> Default for LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    fn default() -> Self {
        Self {
            encoding: Default::default(),
            http_client: None,
            datadog_uri: None,
            default_api_key: None,
            compression: None,
            timeout: None,
            bytes_stored_limit: u64::max_value(),
            log_schema_message_key: None,
            log_schema_timestamp_key: None,
            log_schema_host_key: None,
        }
    }
}

impl<Client> LogApiBuilder<Client>
where
    Client: Service<Request<Body>> + Send + Unpin,
    Client::Future: Send,
    Client::Response: Send,
    Client::Error: Send,
{
    pub fn log_schema(mut self, log_schema: &'static LogSchema) -> Self {
        self.log_schema_message_key = Some(log_schema.message_key());
        self.log_schema_timestamp_key = Some(log_schema.timestamp_key());
        self.log_schema_host_key = Some(log_schema.host_key());
        self
    }

    pub fn encoding(mut self, encoding: EncodingConfigWithDefault<Encoding>) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn default_api_key(mut self, api_key: Arc<str>) -> Self {
        self.default_api_key = Some(api_key);
        self
    }

    pub fn bytes_stored_limit(mut self, limit: u64) -> Self {
        self.bytes_stored_limit = limit;
        self
    }

    // TODO enable and set from config
    // pub fn batch_timeout(mut self, timeout: Duration) -> Self {
    //     self.timeout = Some(timeout);
    //     self
    // }

    pub fn http_client(mut self, client: Client) -> Self {
        self.http_client = Some(client);
        self
    }

    pub fn datadog_uri(mut self, uri: Uri) -> Self {
        self.datadog_uri = Some(uri);
        self
    }

    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn build(self) -> Result<LogApi<Client>, BuildError> {
        let log_api = LogApi {
            default_api_key: self.default_api_key.unwrap(),
            bytes_stored_limit: self.bytes_stored_limit as usize,
            compression: self.compression.unwrap_or_default(),
            datadog_uri: self.datadog_uri.ok_or(BuildError::MissingUri)?,
            encoding: self.encoding,
            http_client: self.http_client.ok_or(BuildError::MissingHttpClient)?,
            log_schema_host_key: self.log_schema_host_key.unwrap_or(log_schema().host_key()),
            log_schema_message_key: self
                .log_schema_message_key
                .unwrap_or(log_schema().message_key()),
            log_schema_timestamp_key: self
                .log_schema_timestamp_key
                .unwrap_or(log_schema().timestamp_key()),
            timeout: self.timeout.unwrap_or(Duration::from_secs(60)),
        };
        Ok(log_api)
    }
}
