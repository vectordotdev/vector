use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use http::{HeaderName, HeaderValue, Method, Request, header};
use tower::{Layer, Service};
use vector_lib::configurable::configurable_component;
use warp::{Filter, reply::Response};

/// Cross-origin resource sharing (CORS) configuration.
///
/// Omit this block to disable CORS. Preflight requests do not generate events.
/// CORS is not authentication; use authentication and TLS to protect the endpoint.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {
    /// Origins permitted to send cross-origin requests.
    ///
    /// Each origin must be an HTTP or HTTPS origin, including its port when non-default,
    /// without a path or trailing slash. Use `["*"]` to allow any origin.
    /// Subdomain wildcards and credentialed browser requests are not supported.
    #[configurable(metadata(docs::examples = "https://app.example.com"))]
    pub allowed_origins: Vec<String>,

    /// Additional HTTP headers permitted in cross-origin requests.
    ///
    /// `Content-Type` and `Content-Encoding` are always permitted.
    /// Add `Authorization` here when using HTTP authentication.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "Authorization"))]
    pub allowed_headers: Vec<String>,

    /// Maximum time a browser may cache a successful preflight response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub max_age_secs: Option<u32>,
}

impl CorsConfig {
    pub(crate) fn build(&self, method: Method) -> crate::Result<warp::cors::Cors> {
        let mut cors = warp::cors()
            .allow_method(method)
            .allow_headers(["content-type", "content-encoding"]);

        if self.allowed_origins.iter().any(|origin| origin == "*") {
            if self.allowed_origins.len() != 1 {
                return Err("cors.allowed_origins: `*` must be the only origin".into());
            }
            cors = cors.allow_any_origin();
        } else {
            // Warp's builder panics on invalid header values. Validate all input first,
            // and also reject URLs that are not serialized browser origins.
            for origin in &self.allowed_origins {
                let url = url::Url::parse(origin)
                    .map_err(|_| format!("cors.allowed_origins: invalid origin `{origin}`"))?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_some_and(|host| host.contains('*'))
                    || url.origin().ascii_serialization() != *origin
                    || HeaderValue::from_str(origin).is_err()
                {
                    return Err(format!(
                        "cors.allowed_origins: expected an HTTP or HTTPS origin, got `{origin}`"
                    )
                    .into());
                }
            }
            cors = cors.allow_origins(self.allowed_origins.iter().map(String::as_str));
        }

        for header in &self.allowed_headers {
            let header = HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| format!("cors.allowed_headers: invalid header name `{header}`"))?;
            cors = cors.allow_header(header);
        }
        if let Some(max_age) = self.max_age_secs {
            cors = cors.max_age(max_age);
        }
        Ok(cors.build())
    }
}

/// Apply Warp's CORS policy outside admission, authentication, and body processing.
#[derive(Clone)]
pub(crate) struct CorsLayer(pub(crate) warp::cors::Cors);

impl<S> Layer<S> for CorsLayer {
    type Service = CorsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CorsService {
            inner,
            cors: self.0.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CorsService<S> {
    inner: S,
    cors: warp::cors::Cors,
}

impl<S, B> Service<Request<B>> for CorsService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let cors = self.cors.clone();
        Box::pin(async move {
            // Evaluate the same policy Warp uses for routes, but without passing
            // the request body to Warp or invoking the ingestion service.
            let mut probe = Request::new(http_body_util::Empty::<bytes::Bytes>::new());
            *probe.method_mut() = request.method().clone();
            *probe.headers_mut() = request.headers().clone();
            let preflight = request.method() == Method::OPTIONS
                && request.headers().contains_key(header::ORIGIN)
                && request
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
            if request.method() == Method::OPTIONS && !preflight {
                // Warp treats OPTIONS without a requested method as NotCors.
                // Validate its origin like a normal request, without changing
                // the actual method or body passed to ingestion.
                *probe.method_mut() = Method::GET;
            }
            let mut policy = warp::service(warp::any().map(warp::reply).with(cors));
            let mut response = match policy.call(probe).await {
                Ok(response) => response,
                Err(error) => match error {},
            };
            if preflight || !response.status().is_success() {
                response.headers_mut().append(
                    header::VARY,
                    HeaderValue::from_static(
                        "Origin, Access-Control-Request-Method, Access-Control-Request-Headers",
                    ),
                );
                return Ok(response);
            }
            let mut result = inner.call(request).await?;
            result.headers_mut().extend(response.headers().clone());
            // Warp reflects allowed origins, including for "*". Prevent caches
            // from reusing that response for a different origin.
            result
                .headers_mut()
                .append(header::VARY, HeaderValue::from_static("Origin"));
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use tower::{ServiceExt, service_fn};
    use warp::Reply;

    use super::*;

    const ORIGIN: &str = "https://app.example.com";

    fn config() -> CorsConfig {
        CorsConfig {
            allowed_origins: vec![ORIGIN.into()],
            allowed_headers: vec!["Authorization".into()],
            max_age_secs: Some(3600),
        }
    }

    #[tokio::test]
    async fn cors_ordinary_options_preserve_ingestion_and_origin_validation() {
        let response = request(config(), Method::OPTIONS, Some(ORIGIN), None, None).await;
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        let response = request(
            config(),
            Method::OPTIONS,
            Some("https://other.example.com"),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
    }

    #[cfg(feature = "sources-http_server")]
    #[tokio::test]
    async fn cors_http_server_ingests_ordinary_options_but_not_preflights() {
        use crate::{
            SourceSender,
            config::{SourceConfig, SourceContext},
            test_util::{addr::next_addr, wait_for_tcp},
        };
        use futures::StreamExt;
        use serde_json::json;
        use std::time::Duration;

        let (_guard, address) = next_addr();
        let source_config: Box<dyn SourceConfig> = serde_json::from_value(json!({
            "type": "http_server",
            "address": address,
            "method": "OPTIONS",
            "cors": {"allowed_origins": [ORIGIN]},
            "decoding": {"codec": "bytes"},
        }))
        .unwrap();
        let (sender, receiver) = SourceSender::new_test();
        let mut receiver = Box::pin(receiver);
        let source = source_config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(source);
        wait_for_tcp(address).await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{address}/");

        let preflight = client
            .request(Method::OPTIONS, &url)
            .header(header::ORIGIN, ORIGIN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "OPTIONS")
            .body("preflight must not be ingested\n")
            .send()
            .await
            .unwrap();
        assert_eq!(preflight.status(), http::StatusCode::OK);
        assert!(futures::poll!(receiver.next()).is_pending());

        let response = client
            .request(Method::OPTIONS, &url)
            .header(header::ORIGIN, ORIGIN)
            .body("ordinary OPTIONS\n")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        let event = tokio::time::timeout(Duration::from_secs(5), receiver.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            event.as_log()["message"].to_string_lossy(),
            "ordinary OPTIONS"
        );

        let response = client
            .request(Method::OPTIONS, url)
            .header(header::ORIGIN, "https://other.example.com")
            .body("forbidden OPTIONS\n")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        assert!(futures::poll!(receiver.next()).is_pending());
    }

    #[cfg(any(feature = "sources-http_server", feature = "sources-opentelemetry"))]
    async fn assert_cors_source_respects_connection_age(source_type: &str) {
        use crate::{
            SourceSender,
            config::{SourceConfig, SourceContext},
            test_util::{addr::next_addr, wait_for_tcp},
        };
        use serde_json::json;

        let (_http_guard, address) = next_addr();
        let (_grpc_guard, grpc_address) = next_addr();
        let http_config = json!({
            "address": address,
            "keepalive": {"max_connection_age_secs": 0},
            "cors": {"allowed_origins": [ORIGIN]},
        });
        let source_config = if source_type == "opentelemetry" {
            json!({
                "type": source_type,
                "http": http_config,
                "grpc": {"address": grpc_address},
            })
        } else {
            let mut source_config = http_config;
            source_config["type"] = json!(source_type);
            source_config
        };
        let source_config: Box<dyn SourceConfig> = serde_json::from_value(source_config).unwrap();
        let (sender, _receiver) = SourceSender::new_test();
        let source = source_config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        // Dropping the set also stops the server if an assertion fails.
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(source);
        wait_for_tcp(address).await;

        let client = reqwest::Client::builder()
            .http1_only()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        for (method, origin, status) in [
            (Method::OPTIONS, ORIGIN, http::StatusCode::OK),
            (
                Method::POST,
                "https://other.example.com",
                http::StatusCode::FORBIDDEN,
            ),
        ] {
            let response = client
                .request(method, format!("http://{address}/"))
                .header(header::ORIGIN, origin)
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()[header::CONNECTION], "close");
        }
    }

    #[cfg(feature = "sources-http_server")]
    #[tokio::test]
    async fn cors_http_server_respects_connection_age() {
        assert_cors_source_respects_connection_age("http_server").await;
    }

    #[cfg(feature = "sources-opentelemetry")]
    #[tokio::test]
    async fn cors_opentelemetry_respects_connection_age() {
        assert_cors_source_respects_connection_age("opentelemetry").await;
    }

    async fn request(
        config: CorsConfig,
        method: Method,
        origin: Option<&str>,
        requested_method: Option<&str>,
        requested_headers: Option<&str>,
    ) -> Response {
        // An ingestion error models authentication, decoding, and admission failures.
        let inner = service_fn(|_: Request<()>| async {
            Ok::<_, Infallible>(
                warp::reply::with_status("ingestion reached", http::StatusCode::UNAUTHORIZED)
                    .into_response(),
            )
        });
        let service = CorsLayer(config.build(Method::POST).unwrap()).layer(inner);
        let mut request = Request::builder().method(method);
        for (name, value) in [
            (header::ORIGIN, origin),
            (header::ACCESS_CONTROL_REQUEST_METHOD, requested_method),
            (header::ACCESS_CONTROL_REQUEST_HEADERS, requested_headers),
        ] {
            if let Some(value) = value {
                request = request.header(name, value);
            }
        }
        service.oneshot(request.body(()).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn cors_preflight_bypasses_ingestion() {
        let response = request(
            config(),
            Method::OPTIONS,
            Some(ORIGIN),
            Some("POST"),
            Some("content-type, content-encoding, authorization"),
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS],
            "POST"
        );
        assert_eq!(response.headers()[header::ACCESS_CONTROL_MAX_AGE], "3600");
        assert!(response.headers().contains_key(header::VARY));
    }

    #[tokio::test]
    async fn cors_rejects_disallowed_origins_methods_and_headers() {
        for (origin, method, headers) in [
            ("https://other.example.com", "POST", "content-type"),
            (ORIGIN, "DELETE", "content-type"),
            (ORIGIN, "POST", "x-not-allowed"),
        ] {
            let response = request(
                config(),
                Method::OPTIONS,
                Some(origin),
                Some(method),
                Some(headers),
            )
            .await;
            assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
            assert!(
                !response
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            );
        }
        let response = request(
            config(),
            Method::POST,
            Some("https://other.example.com"),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn cors_preserves_errors_and_server_to_server_requests() {
        let response = request(config(), Method::POST, Some(ORIGIN), None, None).await;
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );

        for method in [Method::POST, Method::OPTIONS] {
            let response = request(config(), method, None, None, None).await;
            assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
            assert!(
                !response
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            );
        }
    }

    #[tokio::test]
    async fn cors_supports_any_origin_and_empty_allowlist() {
        let mut config = config();
        config.allowed_origins = vec!["*".into()];
        let response = request(config.clone(), Method::POST, Some(ORIGIN), None, None).await;
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );

        config.allowed_origins.clear();
        let response = request(config, Method::POST, Some(ORIGIN), None, None).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn cors_rejects_invalid_configuration() {
        for origin in [
            "not an origin",
            "https://app.example.com/path",
            "https://app.example.com/",
            "https://*.example.com",
            "ftp://app.example.com",
            "null",
            "https://app.example.com\r\ninjected: value",
        ] {
            let mut config = config();
            config.allowed_origins = vec![origin.into()];
            assert!(config.build(Method::POST).is_err(), "{origin}");
        }
        let mut config = config();
        config.allowed_origins.push("*".into());
        assert!(config.build(Method::POST).is_err());
        config.allowed_origins = vec![ORIGIN.into()];
        config.allowed_headers = vec!["not a header".into()];
        assert!(config.build(Method::POST).is_err());
    }
}
