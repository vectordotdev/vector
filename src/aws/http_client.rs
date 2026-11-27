//! The AWS SDK HTTP transport: a [`vector_http::HttpClient`] behind the SDK connector API, so that
//! AWS requests use Vector's OpenSSL TLS settings and proxy routing.

use std::{
    borrow::Cow,
    collections::HashMap,
    error::Error,
    fmt, io,
    sync::{PoisonError, RwLock},
    time::Duration,
};

use aws_smithy_async::{
    future::timeout::{TimedOutError, Timeout},
    rt::sleep::{AsyncSleep, SharedAsyncSleep, TokioSleep},
};
use aws_smithy_runtime_api::{
    box_error::BoxError,
    client::{
        connection::{CaptureSmithyConnection, ConnectionMetadata},
        connector_metadata::ConnectorMetadata,
        http::{
            HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings,
            SharedHttpConnector,
        },
        orchestrator::{HttpRequest, HttpResponse},
        result::ConnectorError,
        runtime_components::RuntimeComponents,
    },
};
use aws_smithy_types::{body::SdkBody, retry::ErrorKind};
use http_body_util::BodyExt;
use hyper_util::client::legacy::connect::{CaptureConnection, HttpInfo, capture_connection};
use vector_http::{ConnectionOptions, HttpError};

use crate::{config::ProxyConfig, tls::MaybeTlsSettings};

/// Serves one [`SdkHttpConnector`] per connect and read timeout, like the SDK's own Hyper client.
pub(crate) struct SdkHttpClient {
    /// The client for requests without a connect timeout. Clients with one are derived from it.
    client: vector_http::HttpClient,
    connectors: RwLock<HashMap<CacheKey, SharedHttpConnector>>,
}

impl SdkHttpClient {
    /// Builds the client, so that invalid TLS or proxy settings are reported here rather than by
    /// the first request.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured proxy URL is invalid or the TLS settings cannot build a
    /// connector.
    pub(crate) fn new(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> crate::Result<Self> {
        let client = vector_http::HttpClient::new(tls, proxy)?;
        let connector = SharedHttpConnector::new(SdkHttpConnector {
            client: client.clone(),
            read_timeout: None,
        });

        Ok(Self {
            client,
            connectors: RwLock::new(HashMap::from([(CacheKey::default(), connector)])),
        })
    }

    fn build_connector(&self, key: CacheKey, components: &RuntimeComponents) -> SdkHttpConnector {
        let client = self
            .client
            .with_options(ConnectionOptions::default().connect_timeout(key.connect_timeout));
        let read_timeout = key.read_timeout.map(|duration| ReadTimeout {
            sleep: components
                .sleep_impl()
                .unwrap_or_else(|| SharedAsyncSleep::new(TokioSleep::new())),
            duration,
        });

        SdkHttpConnector {
            client,
            read_timeout,
        }
    }
}

impl fmt::Debug for SdkHttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SdkHttpClient")
            .finish_non_exhaustive()
    }
}

impl HttpClient for SdkHttpClient {
    fn http_connector(
        &self,
        settings: &HttpConnectorSettings,
        components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        let key = CacheKey {
            connect_timeout: settings.connect_timeout(),
            read_timeout: settings.read_timeout(),
        };

        // A panic while holding the lock cannot leave the map half updated, so a poisoned lock is
        // still usable.
        if let Some(connector) = self
            .connectors
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return connector.clone();
        }

        self.connectors
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key)
            .or_insert_with(|| SharedHttpConnector::new(self.build_connector(key, components)))
            .clone()
    }

    fn connector_metadata(&self) -> Option<ConnectorMetadata> {
        Some(ConnectorMetadata::new("hyper", Some(Cow::Borrowed("1.x"))))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
struct CacheKey {
    connect_timeout: Option<Duration>,
    read_timeout: Option<Duration>,
}

#[derive(Clone, Debug)]
struct ReadTimeout {
    sleep: SharedAsyncSleep,
    duration: Duration,
}

/// Sends SDK requests with a [`vector_http::HttpClient`] built for one set of connector settings.
#[derive(Debug)]
struct SdkHttpConnector {
    client: vector_http::HttpClient,
    /// Limits the wait for the response headers.
    read_timeout: Option<ReadTimeout>,
}

impl HttpConnector for SdkHttpConnector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let client = self.client.clone();

        let mut request = match request.try_into_http1x() {
            Ok(request) => request.map(BodyExt::boxed_unsync),
            Err(error) => {
                return HttpConnectorFuture::ready(Err(ConnectorError::user(error.into())));
            }
        };

        if let Err(error) = client.add_proxy_authorization(&mut request) {
            return HttpConnectorFuture::ready(Err(to_connector_error(error)));
        }

        // Lets the SDK poison the connection, for example after a transient error.
        let capture = capture_connection(&mut request);
        if let Some(capture_smithy_connection) =
            request.extensions().get::<CaptureSmithyConnection>()
        {
            capture_smithy_connection
                .set_connection_retriever(move || smithy_connection_metadata(&capture));
        }

        let read_timeout = self.read_timeout.clone();
        HttpConnectorFuture::new(async move {
            let response = client.send(request);
            let response = match read_timeout {
                Some(ReadTimeout { sleep, duration }) => {
                    Timeout::new(response, sleep.sleep(duration))
                        .await
                        .map_err(|TimedOutError| {
                            ConnectorError::timeout(Box::new(ReadTimeoutError(duration)))
                        })?
                }
                None => response.await,
            }
            .map_err(to_connector_error)?;

            HttpResponse::try_from(response.map(SdkBody::from_body_1_x))
                .map_err(|error| ConnectorError::other(error.into(), None))
        })
    }
}

/// Describes the connection that served a request, and poisons it so the pool does not reuse it.
fn smithy_connection_metadata(capture: &CaptureConnection) -> Option<ConnectionMetadata> {
    let connected = capture.connection_metadata();
    let connected = connected.as_ref()?;

    let mut extras = http::Extensions::new();
    connected.get_extras(&mut extras);
    let http_info = extras.get::<HttpInfo>();

    let poison = capture.clone();
    let mut builder = ConnectionMetadata::builder()
        .proxied(connected.is_proxied())
        .poison_fn(move || {
            if let Some(connected) = poison.connection_metadata().as_ref() {
                connected.poison();
            }
        });
    builder
        .set_local_addr(http_info.map(HttpInfo::local_addr))
        .set_remote_addr(http_info.map(HttpInfo::remote_addr));

    Some(builder.build())
}

/// Classifies a failed request like the SDK's Hyper 1 client, so that the SDK treats the same
/// errors as transient.
fn to_connector_error(error: HttpError) -> ConnectorError {
    let into_connector_error = classify(&error);
    into_connector_error(Box::new(error))
}

fn classify(error: &HttpError) -> fn(BoxError) -> ConnectorError {
    // The request could not be routed, for example because of an invalid proxy URL.
    if !error.is_retriable() {
        return ConnectorError::user;
    }

    // The connect timeout of `vector_http`, or an operating system timeout.
    if find_source::<io::Error>(error).is_some_and(|error| error.kind() == io::ErrorKind::TimedOut)
    {
        return ConnectorError::timeout;
    }

    if let Some(hyper_error) = find_source::<hyper::Error>(error) {
        if hyper_error.is_timeout() {
            return ConnectorError::timeout;
        }
        if hyper_error.is_user() {
            return ConnectorError::user;
        }
        if hyper_error.is_closed()
            || hyper_error.is_canceled()
            || find_source::<io::Error>(hyper_error).is_some()
        {
            return ConnectorError::io;
        }
        // S3 sometimes closes the connection before the response is complete.
        if hyper_error.is_incomplete_message() {
            return |error| ConnectorError::other(error, Some(ErrorKind::TransientError));
        }
        // The server is shutting the HTTP/2 connection down or refused the stream before
        // processing it, so the request can be sent again on a new connection.
        if let Some(h2_error) = find_source::<h2::Error>(hyper_error)
            && (h2_error.is_go_away()
                || (h2_error.is_reset() && h2_error.reason() == Some(h2::Reason::REFUSED_STREAM)))
        {
            return ConnectorError::io;
        }
        return |error| ConnectorError::other(error, None);
    }

    if let Some(client_error) = find_source::<hyper_util::client::legacy::Error>(error)
        && (client_error.is_connect() || find_source::<io::Error>(client_error).is_some())
    {
        return ConnectorError::io;
    }

    |error| ConnectorError::other(error, None)
}

/// Returns the first error of type `E` in the source chain of `error`, including `error` itself.
fn find_source<'a, E: Error + 'static>(error: &'a (dyn Error + 'static)) -> Option<&'a E> {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(matching) = error.downcast_ref::<E>() {
            return Some(matching);
        }
        next = error.source();
    }
    None
}

/// The response headers did not arrive within the read timeout.
#[derive(Debug)]
struct ReadTimeoutError(Duration);

impl fmt::Display for ReadTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "HTTP read timeout occurred after {:?}", self.0)
    }
}

impl Error for ReadTimeoutError {
    /// Marks this error as a timeout for code that looks for [`TimedOutError`] in the chain, as
    /// the SDK does.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&TimedOutError)
    }
}

/// Runtime components with the SDK's no-auth, never-retry, and Tokio sleep implementations, so
/// that a connector can be requested from an HTTP client without an SDK service client.
#[cfg(test)]
pub(crate) fn test_runtime_components() -> RuntimeComponents {
    use aws_smithy_runtime::client::{
        auth::no_auth::{NO_AUTH_SCHEME_ID, NoAuthScheme},
        identity::{IdentityCache, no_auth::NoAuthIdentityResolver},
        orchestrator::endpoints::StaticUriEndpointResolver,
        retries::strategy::NeverRetryStrategy,
    };
    use aws_smithy_runtime_api::client::{
        auth::static_resolver::StaticAuthSchemeOptionResolver,
        runtime_components::RuntimeComponentsBuilder,
    };

    RuntimeComponentsBuilder::new("aws::tests")
        .with_auth_scheme(NoAuthScheme::new())
        .with_auth_scheme_option_resolver(Some(StaticAuthSchemeOptionResolver::new(vec![
            NO_AUTH_SCHEME_ID,
        ])))
        .with_endpoint_resolver(Some(StaticUriEndpointResolver::http_localhost(0)))
        .with_identity_cache(Some(IdentityCache::no_cache()))
        .with_identity_resolver(NO_AUTH_SCHEME_ID, NoAuthIdentityResolver::new())
        .with_retry_strategy(Some(NeverRetryStrategy::new()))
        .with_sleep_impl(Some(TokioSleep::new()))
        .build()
        .expect("all required runtime components are set")
}
