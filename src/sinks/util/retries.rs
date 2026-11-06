use std::{
    borrow::Cow,
    cmp,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::FutureExt;
use tokio::time::{Sleep, sleep};
use tower::{retry::Policy, timeout::error::Elapsed};
use vector_lib::configurable::configurable_component;

use crate::Error;

const ERROR_LOG_RETRY_ATTEMPT_INTERVAL: usize = 10;

pub enum RetryAction<Request = ()> {
    /// Indicate that this request should be retried with a reason
    Retry(Cow<'static, str>),
    /// Indicate that a portion of this request should be retried with a generic function
    RetryPartial(Box<dyn Fn(Request) -> Request + Send + Sync>),
    /// Indicate that this request should not be retried with a reason
    DontRetry(Cow<'static, str>),
    /// Indicate that this request should not be retried but the request was successful
    Successful,
}

pub trait RetryLogic: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    type Request;
    type Response;

    /// When the Service call returns an `Err` response, this function allows
    /// implementors to specify what kinds of errors can be retried.
    fn is_retriable_error(&self, error: &Self::Error) -> bool;

    /// When the Service call times out, this function allows implementors to
    /// specify if the timeout should be retried.
    fn is_retriable_timeout(&self) -> bool {
        true
    }

    /// When the Service call returns an `Ok` response, this function allows
    /// implementors to specify additional logic to determine if the success response
    /// is actually an error. This is particularly useful when the downstream service
    /// of a sink returns a transport protocol layer success but error data in the
    /// response body. For example, an HTTP 200 status, but the body of the response
    /// contains a list of errors encountered while processing.
    fn should_retry_response(&self, _response: &Self::Response) -> RetryAction<Self::Request> {
        // Treat the default as the request is successful
        RetryAction::Successful
    }

    /// Optional hook run when an error is determined to be retriable.
    fn on_retriable_error(&self, _error: &Self::Error) {}
}

/// The jitter mode to use for retry backoff behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
pub enum JitterMode {
    /// No jitter.
    None,

    /// Full jitter.
    ///
    /// The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
    /// strategy.
    ///
    /// Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
    /// of creating accidental denial of service (`DoS`) conditions against your own systems when
    /// many clients are recovering from a failure state.
    #[default]
    Full,
}

#[derive(Debug, Clone)]
pub struct FibonacciRetryPolicy<L> {
    remaining_attempts: usize,
    retry_attempt: usize,
    previous_duration: Duration,
    current_duration: Duration,
    jitter_mode: JitterMode,
    current_jitter_duration: Duration,
    max_duration: Duration,
    logic: L,
}

pub struct RetryPolicyFuture {
    delay: Pin<Box<Sleep>>,
}

impl<L: RetryLogic> FibonacciRetryPolicy<L> {
    pub fn new(
        remaining_attempts: usize,
        initial_backoff: Duration,
        max_duration: Duration,
        logic: L,
        jitter_mode: JitterMode,
    ) -> Self {
        FibonacciRetryPolicy {
            remaining_attempts,
            retry_attempt: 0,
            previous_duration: Duration::from_secs(0),
            current_duration: initial_backoff,
            jitter_mode,
            current_jitter_duration: Self::add_full_jitter(initial_backoff),
            max_duration,
            logic,
        }
    }

    fn add_full_jitter(d: Duration) -> Duration {
        // https://github.com/vectordotdev/vector/issues/23659
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Preserve jitter conversion."
        )]
        let millis = d.as_millis() as u64;
        let jitter = (rand::random::<u64>() % millis) + 1;
        Duration::from_millis(jitter)
    }

    const fn backoff(&self) -> Duration {
        match self.jitter_mode {
            JitterMode::None => self.current_duration,
            JitterMode::Full => self.current_jitter_duration,
        }
    }

    fn advance(&mut self) {
        let sum = self
            .previous_duration
            .checked_add(self.current_duration)
            .unwrap_or(Duration::MAX);
        let next_duration = cmp::min(sum, self.max_duration);
        self.remaining_attempts = self.remaining_attempts.saturating_sub(1);
        self.retry_attempt = self.retry_attempt.saturating_add(1);
        self.previous_duration = self.current_duration;
        self.current_duration = next_duration;
        self.current_jitter_duration = Self::add_full_jitter(next_duration);
    }

    fn build_retry(&mut self) -> RetryPolicyFuture {
        self.advance();
        let delay = Box::pin(sleep(self.backoff()));

        debug!(
            message = "Retrying request.",
            retry_attempt = self.retry_attempt,
            delay_ms = %self.backoff().as_millis(),
        );
        RetryPolicyFuture { delay }
    }

    const fn next_retry_attempt(&self) -> usize {
        self.retry_attempt.saturating_add(1)
    }

    const fn should_emit_sustained_failure(&self) -> bool {
        self.next_retry_attempt()
            .is_multiple_of(ERROR_LOG_RETRY_ATTEMPT_INTERVAL)
    }

    fn log_sustained_failure(&self) {
        if self.should_emit_sustained_failure() {
            error!(
                message = "Sustained request failures.",
                retry_attempt = self.next_retry_attempt(),
            );
        }
    }

    fn log_response_retry(&self, reason: &str) {
        let retry_attempt = self.next_retry_attempt();
        warn!(message = "Retrying after response.", reason, retry_attempt,);
        self.log_sustained_failure();
    }

    fn log_partial_response_retry(&self) {
        let retry_attempt = self.next_retry_attempt();
        warn!(
            message = "OK/retrying partial after response.",
            retry_attempt,
        );
        self.log_sustained_failure();
    }

    fn log_error_retry(&self, error: &L::Error) {
        let retry_attempt = self.next_retry_attempt();
        warn!(
            message = "Retrying after error.",
            %error,
            retry_attempt,
        );
        self.log_sustained_failure();
    }

    fn log_timeout_retry(&self) {
        let retry_attempt = self.next_retry_attempt();
        warn!(
            message = "Request timed out. If this happens often while the events are actually reaching their destination, try decreasing `batch.max_bytes` and/or using `compression` if applicable. Alternatively `request.timeout_secs` can be increased.",
            retry_attempt,
        );
        self.log_sustained_failure();
    }
}

impl<Req, Res, L> Policy<Req, Res, Error> for FibonacciRetryPolicy<L>
where
    Req: Clone + Send + 'static,
    L: RetryLogic<Request = Req, Response = Res>,
{
    type Future = RetryPolicyFuture;

    // NOTE: in the error cases- `Error` and `EventsDropped` internal events are emitted by the
    // driver, so only need to log here.
    fn retry(&mut self, req: &mut Req, result: &mut Result<Res, Error>) -> Option<Self::Future> {
        match result {
            Ok(response) => match self.logic.should_retry_response(response) {
                RetryAction::Retry(reason) => {
                    if self.remaining_attempts == 0 {
                        error!(
                            message = "OK/retry response but retries exhausted; dropping the request.",
                            reason = ?reason,
                        );
                        return None;
                    }

                    self.log_response_retry(&reason);
                    Some(self.build_retry())
                }
                RetryAction::RetryPartial(modify_request) => {
                    if self.remaining_attempts == 0 {
                        error!(
                            message =
                                "OK/retry response but retries exhausted; dropping the request.",
                        );
                        return None;
                    }
                    *req = modify_request(req.clone());
                    self.log_partial_response_retry();
                    Some(self.build_retry())
                }
                RetryAction::DontRetry(reason) => {
                    error!(message = "Not retriable; dropping the request.", ?reason);
                    None
                }

                RetryAction::Successful => None,
            },
            Err(error) => {
                if self.remaining_attempts == 0 {
                    error!(message = "Retries exhausted; dropping the request.", %error);
                    return None;
                }

                if let Some(expected) = error.downcast_ref::<L::Error>() {
                    if self.logic.is_retriable_error(expected) {
                        self.logic.on_retriable_error(expected);
                        self.log_error_retry(expected);
                        Some(self.build_retry())
                    } else {
                        error!(
                            message = "Non-retriable error; dropping the request.",
                            %error,
                        );
                        None
                    }
                } else if error.downcast_ref::<Elapsed>().is_some() {
                    if self.logic.is_retriable_timeout() {
                        self.log_timeout_retry();
                        Some(self.build_retry())
                    } else {
                        error!(
                            message =
                                "Request timed out and is not retriable; dropping the request."
                        );
                        None
                    }
                } else {
                    error!(
                        message = "Unexpected error type; dropping the request.",
                        %error
                    );
                    None
                }
            }
        }
    }

    fn clone_request(&mut self, request: &Req) -> Option<Req> {
        Some(request.clone())
    }
}

// Safety: `L` is never pinned and we use no unsafe pin projections
// therefore this safe.
impl Unpin for RetryPolicyFuture {}

impl Future for RetryPolicyFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        std::task::ready!(self.delay.poll_unpin(cx));
        Poll::Ready(())
    }
}

impl<Request> RetryAction<Request> {
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, RetryAction::Retry(_) | RetryAction::RetryPartial(_))
    }

    #[must_use]
    pub const fn is_not_retryable(&self) -> bool {
        matches!(self, RetryAction::DontRetry(_))
    }

    #[must_use]
    pub const fn is_successful(&self) -> bool {
        matches!(self, RetryAction::Successful)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt,
        io::{Read, Seek, SeekFrom},
        time::Duration,
    };

    use tokio::time;
    use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok, task};
    use tower::retry::RetryLayer;
    use tower_test::{assert_request_eq, mock};

    use super::*;
    use crate::test_util::trace_init;

    #[tokio::test]
    async fn service_error_retry() {
        trace_init();

        time::pause();

        let policy = FibonacciRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
            JitterMode::None,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let fut = svc.call("hello");
        let mut fut = task::spawn(fut);

        assert_request_eq!(handle, "hello").send_error(Error(true));

        assert_pending!(fut.poll());

        time::advance(Duration::from_secs(2)).await;
        assert_pending!(fut.poll());

        assert_request_eq!(handle, "hello").send_response("world");
        assert_eq!(fut.await.unwrap(), "world");
    }

    #[tokio::test]
    async fn service_error_no_retry() {
        trace_init();

        let policy = FibonacciRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
            JitterMode::None,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let mut fut = task::spawn(svc.call("hello"));
        assert_request_eq!(handle, "hello").send_error(Error(false));
        assert_ready_err!(fut.poll());
    }

    #[tokio::test]
    async fn timeout_error() {
        trace_init();

        time::pause();

        let policy = FibonacciRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
            JitterMode::None,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let mut fut = task::spawn(svc.call("hello"));
        assert_request_eq!(handle, "hello").send_error(Elapsed::new());
        assert_pending!(fut.poll());

        time::advance(Duration::from_secs(2)).await;
        assert_pending!(fut.poll());

        assert_request_eq!(handle, "hello").send_response("world");
        assert_eq!(fut.await.unwrap(), "world");
    }

    #[tokio::test]
    async fn timeout_error_no_retry() {
        trace_init();

        let policy = FibonacciRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            NoTimeoutRetryLogic,
            JitterMode::None,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let mut fut = task::spawn(svc.call("hello"));
        assert_request_eq!(handle, "hello").send_error(Elapsed::new());
        assert_ready_err!(fut.poll());
    }

    #[tokio::test]
    async fn response_retry_logs_attempt_number() {
        let events = response_retry_log_events(2);
        let events = events_with_message(&events, "Retrying after response.");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["fields"]["retry_attempt"], 1);
        assert_eq!(events[1]["fields"]["retry_attempt"], 2);
    }

    #[tokio::test]
    async fn sustained_response_retries_emit_safe_periodic_errors() {
        let events = response_retry_log_events(20);
        let retry_events = events_with_message(&events, "Retrying after response.");
        let sustained_failure_events = events_with_message(&events, "Sustained request failures.");

        assert_eq!(retry_events.len(), 20);
        assert!(retry_events.iter().all(|event| event["level"] == "WARN"));
        assert_eq!(sustained_failure_events.len(), 2);
        assert_eq!(sustained_failure_events[0]["level"], "ERROR");
        assert_eq!(sustained_failure_events[0]["fields"]["retry_attempt"], 10);
        assert!(sustained_failure_events[0]["fields"]["reason"].is_null());
        assert_eq!(sustained_failure_events[1]["level"], "ERROR");
        assert_eq!(sustained_failure_events[1]["fields"]["retry_attempt"], 20);
        assert!(sustained_failure_events[1]["fields"]["reason"].is_null());
    }

    fn response_retry_log_events(retries: usize) -> Vec<serde_json::Value> {
        let mut logs = tempfile::tempfile().unwrap();
        let subscriber = tracing::Dispatch::new(
            tracing_subscriber::fmt()
                .json()
                .with_max_level(tracing::Level::WARN)
                .with_writer(logs.try_clone().unwrap())
                .finish(),
        );
        let mut policy = FibonacciRetryPolicy::new(
            retries,
            Duration::from_secs(1),
            Duration::from_secs(10),
            ResponseRetryLogic,
            JitterMode::None,
        );

        tracing::dispatcher::with_default(&subscriber, || {
            for _ in 0..retries {
                let mut request = "hello";
                let mut response = Ok("world");
                assert!(policy.retry(&mut request, &mut response).is_some());
            }
        });

        logs.seek(SeekFrom::Start(0)).unwrap();
        let mut output = String::new();
        logs.read_to_string(&mut output).unwrap();
        output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn events_with_message<'a>(
        events: &'a [serde_json::Value],
        message: &str,
    ) -> Vec<&'a serde_json::Value> {
        events
            .iter()
            .filter(|event| event["fields"]["message"] == message)
            .collect()
    }

    #[test]
    fn backoff_grows_to_max() {
        let mut policy = FibonacciRetryPolicy::new(
            10,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
            JitterMode::None,
        );
        assert_eq!(Duration::from_secs(1), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(1), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(2), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(3), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(5), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(8), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(10), policy.backoff());

        policy.advance();
        assert_eq!(Duration::from_secs(10), policy.backoff());
    }

    #[test]
    fn backoff_grows_to_max_with_jitter() {
        let max_duration = Duration::from_secs(10);
        let mut policy = FibonacciRetryPolicy::new(
            10,
            Duration::from_secs(1),
            max_duration,
            SvcRetryLogic,
            JitterMode::Full,
        );

        let expected_fib = [1, 1, 2, 3, 5, 8];

        for (i, &exp_fib_secs) in expected_fib.iter().enumerate() {
            let backoff = policy.backoff();
            let upper_bound = Duration::from_secs(exp_fib_secs);

            // Check if the backoff is within the expected range, considering the jitter
            assert!(
                !backoff.is_zero() && backoff <= upper_bound,
                "Attempt {}: Expected backoff to be within 0 and {upper_bound:?}, got {backoff:?}",
                i + 1
            );

            policy.advance();
        }

        // Once the max backoff is reached, it should not exceed the max backoff.
        for _ in 0..4 {
            let backoff = policy.backoff();
            assert!(
                !backoff.is_zero() && backoff <= max_duration,
                "Expected backoff to not exceed {max_duration:?}, got {backoff:?}"
            );

            policy.advance();
        }
    }

    #[derive(Debug, Clone)]
    struct SvcRetryLogic;

    impl RetryLogic for SvcRetryLogic {
        type Error = Error;
        type Request = &'static str;
        type Response = &'static str;

        fn is_retriable_error(&self, error: &Self::Error) -> bool {
            error.0
        }
    }

    #[derive(Debug, Clone)]
    struct NoTimeoutRetryLogic;

    impl RetryLogic for NoTimeoutRetryLogic {
        type Error = Error;
        type Request = &'static str;
        type Response = &'static str;

        fn is_retriable_error(&self, error: &Self::Error) -> bool {
            error.0
        }

        fn is_retriable_timeout(&self) -> bool {
            false
        }
    }

    #[derive(Debug, Clone)]
    struct ResponseRetryLogic;

    impl RetryLogic for ResponseRetryLogic {
        type Error = Error;
        type Request = &'static str;
        type Response = &'static str;

        fn is_retriable_error(&self, error: &Self::Error) -> bool {
            error.0
        }

        fn should_retry_response(&self, _response: &Self::Response) -> RetryAction<Self::Request> {
            RetryAction::Retry("test response".into())
        }
    }

    #[derive(Debug)]
    struct Error(bool);

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error")
        }
    }

    impl std::error::Error for Error {}
}
