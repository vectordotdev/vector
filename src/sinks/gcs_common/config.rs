use std::marker::PhantomData;

use futures::FutureExt;
use http::{StatusCode, Uri};
use hyper::Body;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;

use crate::{
    gcp::{GcpAuthenticator, GcpError},
    http::{HttpClient, HttpError},
    sinks::{
        Healthcheck, HealthcheckError,
        gcs_common::service::GcsResponse,
        util::{
            http::{HttpResponse, RetryStrategy},
            retries::{RetryAction, RetryLogic},
        },
    },
};

pub fn default_endpoint() -> String {
    "https://storage.googleapis.com".to_string()
}

/// GCS Predefined ACLs.
///
/// For more information, see [Predefined ACLs][predefined_acls].
///
/// [predefined_acls]: https://cloud.google.com/storage/docs/access-control/lists#predefined-acl
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GcsPredefinedAcl {
    /// Bucket/object can be read by authenticated users.
    ///
    /// The bucket/object owner is granted the `OWNER` permission, and anyone authenticated Google
    /// account holder is granted the `READER` permission.
    AuthenticatedRead,

    /// Object is semi-private.
    ///
    /// Both the object owner and bucket owner are granted the `OWNER` permission.
    ///
    /// Only relevant when specified for an object: this predefined ACL is otherwise ignored when
    /// specified for a bucket.
    BucketOwnerFullControl,

    /// Object is private, except to the bucket owner.
    ///
    /// The object owner is granted the `OWNER` permission, and the bucket owner is granted the
    /// `READER` permission.
    ///
    /// Only relevant when specified for an object: this predefined ACL is otherwise ignored when
    /// specified for a bucket.
    BucketOwnerRead,

    /// Bucket/object are private.
    ///
    /// The bucket/object owner is granted the `OWNER` permission, and no one else has
    /// access.
    Private,

    /// Bucket/object are private within the project.
    ///
    /// Project owners and project editors are granted the `OWNER` permission, and anyone who is
    /// part of the project team is granted the `READER` permission.
    ///
    /// This is the default.
    #[default]
    ProjectPrivate,

    /// Bucket/object can be read publicly.
    ///
    /// The bucket/object owner is granted the `OWNER` permission, and all other users, whether
    /// authenticated or anonymous, are granted the `READER` permission.
    PublicRead,
}

/// GCS storage classes.
///
/// For more information, see [Storage classes][storage_classes].
///
/// [storage_classes]: https://cloud.google.com/storage/docs/storage-classes
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GcsStorageClass {
    /// Standard storage.
    ///
    /// This is the default.
    #[default]
    Standard,

    /// Nearline storage.
    Nearline,

    /// Coldline storage.
    Coldline,

    /// Archive storage.
    Archive,
}

#[derive(Debug, Snafu)]
pub enum GcsError {
    #[snafu(display("Bucket {:?} not found", bucket))]
    BucketNotFound { bucket: String },
}

pub fn build_healthcheck(
    bucket: String,
    client: HttpClient,
    base_url: String,
    auth: GcpAuthenticator,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let uri = base_url.parse::<Uri>()?;
        let mut request = http::Request::head(uri).body(Body::empty())?;

        auth.apply(&mut request);

        let not_found_error = GcsError::BucketNotFound { bucket }.into();

        let response = client.send(request).await?;
        healthcheck_response(response, not_found_error)
    };

    Ok(healthcheck.boxed())
}

pub fn healthcheck_response(
    response: http::Response<hyper::Body>,
    not_found_error: crate::Error,
) -> crate::Result<()> {
    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::FORBIDDEN => Err(GcpError::HealthcheckForbidden.into()),
        StatusCode::NOT_FOUND => Err(not_found_error),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

pub struct GcsRetryLogic<Request> {
    request: PhantomData<Request>,
    /// Optional auth handle. When present, a 401 response fires a (throttled)
    /// background credential rebuild before the standard retry is re-issued,
    /// converting a permanent stale-token state into a one-shot self-heal.
    auth: Option<GcpAuthenticator>,
}

impl<Request> GcsRetryLogic<Request> {
    pub const fn with_auth(auth: GcpAuthenticator) -> Self {
        Self {
            request: PhantomData,
            auth: Some(auth),
        }
    }
}

impl<Request> Default for GcsRetryLogic<Request> {
    fn default() -> Self {
        Self {
            request: PhantomData,
            auth: None,
        }
    }
}

impl<Request> Clone for GcsRetryLogic<Request> {
    fn clone(&self) -> Self {
        Self {
            request: PhantomData,
            auth: self.auth.clone(),
        }
    }
}

// This is a clone of HttpRetryLogic for the Body type, should get merged
impl<Request: Clone + Send + Sync + 'static> RetryLogic for GcsRetryLogic<Request> {
    type Error = hyper::Error;
    type Request = Request;
    type Response = GcsResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        let status = response.inner.status();

        match status {
            StatusCode::UNAUTHORIZED => {
                // Synchronously wait for the refresh so the retry uses the
                // fresh token instead of burning a retry slot on the stale
                // one. See `GcpAuthRetryLogic::should_retry_response` for
                // the full rationale (block_in_place + in-flight coalescing).
                if let Some(auth) = &self.auth {
                    let auth = auth.clone();
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async move {
                            auth.force_refresh().await;
                        });
                    });
                }
                RetryAction::Retry("unauthorized".into())
            }
            StatusCode::REQUEST_TIMEOUT => RetryAction::Retry("request timeout".into()),
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(status.to_string().into()),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {status}").into()),
        }
    }
}

/// Retry logic for GCP sinks that go through the generic `HttpService` /
/// `BatchedHttpSink` machinery rather than `GcsService`. Adds 401-driven
/// credential self-heal to a base `RetryStrategy`. Generic over the response
/// type via a status-extractor closure so it can wrap both
/// `http::Response<Bytes>` (BatchedHttpSink) and `HttpResponse` (HttpService).
pub struct GcpAuthRetryLogic<F, Req, Res> {
    get_status: F,
    retry_strategy: RetryStrategy,
    auth: GcpAuthenticator,
    _markers: PhantomData<(Req, Res)>,
}

impl<F, Req, Res> GcpAuthRetryLogic<F, Req, Res> {
    // `const fn` because clippy prefers it and there's no downside — it
    // remains callable from non-const contexts. The convenience helpers
    // below pass an `impl Fn` for `get_status` and aren't themselves const,
    // so this isn't actually invokable in const code today; the qualifier
    // is purely forward-compatible.
    pub const fn new(get_status: F, retry_strategy: RetryStrategy, auth: GcpAuthenticator) -> Self {
        Self {
            get_status,
            retry_strategy,
            auth,
            _markers: PhantomData,
        }
    }
}

/// Convenience constructor for sinks whose tower service returns
/// `http::Response<Bytes>` (i.e. `BatchedHttpSink`).
pub fn gcp_hyper_response_retry_logic<Req: Clone + Send + Sync + 'static>(
    retry_strategy: RetryStrategy,
    auth: GcpAuthenticator,
) -> GcpAuthRetryLogic<
    impl Fn(&http::Response<bytes::Bytes>) -> StatusCode + Clone + Send + Sync + 'static,
    Req,
    http::Response<bytes::Bytes>,
> {
    GcpAuthRetryLogic::new(
        |r: &http::Response<bytes::Bytes>| r.status(),
        retry_strategy,
        auth,
    )
}

/// Convenience constructor for sinks whose tower service returns
/// `HttpResponse` (i.e. the modern `HttpService`-based stream sinks).
pub fn gcp_http_response_retry_logic<Req: Clone + Send + Sync + 'static>(
    retry_strategy: RetryStrategy,
    auth: GcpAuthenticator,
) -> GcpAuthRetryLogic<
    impl Fn(&HttpResponse) -> StatusCode + Clone + Send + Sync + 'static,
    Req,
    HttpResponse,
> {
    GcpAuthRetryLogic::new(
        |r: &HttpResponse| r.http_response.status(),
        retry_strategy,
        auth,
    )
}

impl<F: Clone, Req, Res> Clone for GcpAuthRetryLogic<F, Req, Res> {
    fn clone(&self) -> Self {
        Self {
            get_status: self.get_status.clone(),
            retry_strategy: self.retry_strategy.clone(),
            auth: self.auth.clone(),
            _markers: PhantomData,
        }
    }
}

impl<F, Req, Res> RetryLogic for GcpAuthRetryLogic<F, Req, Res>
where
    F: Fn(&Res) -> StatusCode + Clone + Send + Sync + 'static,
    Req: Clone + Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    type Error = HttpError;
    type Request = Req;
    type Response = Res;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if self.retry_strategy == RetryStrategy::None {
            false
        } else {
            error.is_retriable()
        }
    }

    fn is_retriable_timeout(&self) -> bool {
        self.retry_strategy != RetryStrategy::None
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        let status = (self.get_status)(response);
        if status == StatusCode::UNAUTHORIZED {
            // Decide whether 401 self-heals. `Default` historically returned
            // DontRetry on 401; we override because 401 typically indicates a
            // stale token and the next attempt with a fresh token will succeed
            // (this matches GcsRetryLogic). `All` already retries everything,
            // so we just add the refresh. `Custom` is an explicit user choice
            // — honor it. `None` means "do not retry any errors" — honor it,
            // and skip the refresh since the failed request will not be
            // re-issued.
            let self_heal = match &self.retry_strategy {
                RetryStrategy::None => false,
                RetryStrategy::Default | RetryStrategy::All => true,
                RetryStrategy::Custom { status_codes } => {
                    status_codes.contains(&StatusCode::UNAUTHORIZED)
                }
            };
            if self_heal {
                // Synchronously wait for the refresh so the retry uses a
                // fresh token rather than burning a retry slot on the same
                // stale token. `block_in_place` releases this worker thread
                // for other tasks during the wait; `force_refresh`'s
                // in-flight guard coalesces concurrent 401s so the actual
                // STS call happens at most once per burst.
                //
                // Requires the multi-thread tokio runtime (Vector's CLI
                // configures this); tests exercising this path must use
                // `#[tokio::test(flavor = "multi_thread")]`.
                let auth = self.auth.clone();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async move {
                        auth.force_refresh().await;
                    });
                });
                return RetryAction::Retry("unauthorized".into());
            }
        }
        self.retry_strategy.retry_action(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    /// Helper that maps a `RetryAction` variant to a short tag so we can
    /// table-test cleanly. The `Cow<'static, str>` reasons aren't part of the
    /// behavior contract we're testing — only which arm fired.
    #[derive(Debug, PartialEq, Eq)]
    enum Action {
        Retry,
        DontRetry,
        Successful,
    }

    fn classify<R>(action: &RetryAction<R>) -> Action {
        match action {
            RetryAction::Retry(_) => Action::Retry,
            RetryAction::RetryPartial(_) => Action::Retry,
            RetryAction::DontRetry(_) => Action::DontRetry,
            RetryAction::Successful => Action::Successful,
        }
    }

    fn response(status: StatusCode) -> http::Response<Bytes> {
        http::Response::builder()
            .status(status)
            .body(Bytes::new())
            .unwrap()
    }

    // Verifies the (HTTP status × RetryStrategy) decision matrix in
    // `GcpAuthRetryLogic::should_retry_response`. The auth handle is
    // `GcpAuthenticator::None`, which makes `force_refresh` a no-op — the
    // matrix only asserts which `RetryAction` variant fires, not the side
    // effect.
    //
    // `multi_thread` flavor: the self-heal path uses `block_in_place`, which
    // panics on a current-thread runtime.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn retry_logic_matrix() {
        let auth = GcpAuthenticator::None;
        let custom_401 = RetryStrategy::Custom {
            status_codes: vec![StatusCode::UNAUTHORIZED],
        };
        let custom_500 = RetryStrategy::Custom {
            status_codes: vec![StatusCode::INTERNAL_SERVER_ERROR],
        };

        let cases: Vec<(StatusCode, RetryStrategy, Action)> = vec![
            // 200 OK always classifies as Successful regardless of strategy.
            (StatusCode::OK, RetryStrategy::None, Action::Successful),
            (StatusCode::OK, RetryStrategy::Default, Action::Successful),
            (StatusCode::OK, RetryStrategy::All, Action::Successful),
            // 401: self-heal kicks in for Default/All and for Custom that
            // explicitly lists 401. None and Custom-without-401 honor the
            // user choice and do NOT retry.
            (
                StatusCode::UNAUTHORIZED,
                RetryStrategy::None,
                Action::DontRetry,
            ),
            (
                StatusCode::UNAUTHORIZED,
                RetryStrategy::Default,
                Action::Retry,
            ),
            (StatusCode::UNAUTHORIZED, RetryStrategy::All, Action::Retry),
            (StatusCode::UNAUTHORIZED, custom_401.clone(), Action::Retry),
            (
                StatusCode::UNAUTHORIZED,
                custom_500.clone(),
                Action::DontRetry,
            ),
            // 5xx: defer to strategy. Default retries 5xx; None doesn't;
            // Custom honors its list.
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                RetryStrategy::None,
                Action::DontRetry,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                RetryStrategy::Default,
                Action::Retry,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                RetryStrategy::All,
                Action::Retry,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                custom_401.clone(),
                Action::DontRetry,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                custom_500.clone(),
                Action::Retry,
            ),
            // 404 (a non-401 4xx): never retried by Default. Custom honors
            // its list. All retries everything non-2xx.
            (
                StatusCode::NOT_FOUND,
                RetryStrategy::None,
                Action::DontRetry,
            ),
            (
                StatusCode::NOT_FOUND,
                RetryStrategy::Default,
                Action::DontRetry,
            ),
            (StatusCode::NOT_FOUND, RetryStrategy::All, Action::Retry),
            (StatusCode::NOT_FOUND, custom_401, Action::DontRetry),
            (StatusCode::NOT_FOUND, custom_500, Action::DontRetry),
        ];

        for (status, strategy, expected) in cases {
            let logic = gcp_hyper_response_retry_logic::<Bytes>(strategy.clone(), auth.clone());
            let action = logic.should_retry_response(&response(status));
            assert_eq!(
                classify(&action),
                expected,
                "status={status}, strategy={strategy:?}",
            );
        }
    }
}
