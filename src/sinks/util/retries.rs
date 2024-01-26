use std::{
    borrow::Cow,
    cmp,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::FutureExt;
use tokio::time::{sleep, Sleep};
use tower::{retry::Policy, timeout::error::Elapsed};
use vector_lib::configurable::configurable_component;

use crate::Error;

pub enum RetryAction {
    /// Indicate that this request should be retried with a reason
    Retry(Cow<'static, str>),
    /// Indicate that this request should not be retried with a reason
    DontRetry(Cow<'static, str>),
    /// Indicate that this request should not be retried but the request was successful
    Successful,
}

pub trait RetryLogic: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    type Response;

    /// When the Service call returns an `Err` response, this function allows
    /// implementors to specify what kinds of errors can be retried.
    fn is_retriable_error(&self, error: &Self::Error) -> bool;

    /// When the Service call returns an `Ok` response, this function allows
    /// implementors to specify additional logic to determine if the success response
    /// is actually an error. This is particularly useful when the downstream service
    /// of a sink returns a transport protocol layer success but error data in the
    /// response body. For example, an HTTP 200 status, but the body of the response
    /// contains a list of errors encountered while processing.
    fn should_retry_response(&self, _response: &Self::Response) -> RetryAction {
        // Treat the default as the request is successful
        RetryAction::Successful
    }
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
    /// of creating accidental denial of service (DoS) conditions against your own systems when
    /// many clients are recovering from a failure state.
    #[default]
    Full,
}

#[derive(Debug, Clone)]
pub struct FibonacciRetryPolicy<L> {
    remaining_attempts: usize,
    previous_duration: Duration,
    current_duration: Duration,
    jitter_mode: JitterMode,
    current_jitter_duration: Duration,
    max_duration: Duration,
    logic: L,
}

pub struct RetryPolicyFuture<L: RetryLogic> {
    delay: Pin<Box<Sleep>>,
    policy: FibonacciRetryPolicy<L>,
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
            previous_duration: Duration::from_secs(0),
            current_duration: initial_backoff,
            jitter_mode,
            current_jitter_duration: Self::add_full_jitter(initial_backoff),
            max_duration,
            logic,
        }
    }

    fn add_full_jitter(d: Duration) -> Duration {
        let jitter = (rand::random::<u64>() % (d.as_millis() as u64)) + 1;
        Duration::from_millis(jitter)
    }

    fn advance(&self) -> FibonacciRetryPolicy<L> {
        let next_duration: Duration = cmp::min(
            self.previous_duration + self.current_duration,
            self.max_duration,
        );

        FibonacciRetryPolicy {
            remaining_attempts: self.remaining_attempts - 1,
            previous_duration: self.current_duration,
            current_duration: next_duration,
            current_jitter_duration: Self::add_full_jitter(next_duration),
            jitter_mode: self.jitter_mode,
            max_duration: self.max_duration,
            logic: self.logic.clone(),
        }
    }

    const fn backoff(&self) -> Duration {
        match self.jitter_mode {
            JitterMode::None => self.current_duration,
            JitterMode::Full => self.current_jitter_duration,
        }
    }

    fn build_retry(&self) -> RetryPolicyFuture<L> {
        let policy = self.advance();
        let delay = Box::pin(sleep(self.backoff()));

        debug!(message = "Retrying request.", delay_ms = %self.backoff().as_millis());
        RetryPolicyFuture { delay, policy }
    }
}

impl<Req, Res, L> Policy<Req, Res, Error> for FibonacciRetryPolicy<L>
where
    Req: Clone,
    L: RetryLogic<Response = Res>,
{
    type Future = RetryPolicyFuture<L>;

    // NOTE: in the error cases- `Error` and `EventsDropped` internal events are emitted by the
    // driver, so only need to log here.
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        match result {
            Ok(response) => match self.logic.should_retry_response(response) {
                RetryAction::Retry(reason) => {
                    if self.remaining_attempts == 0 {
                        error!(
                            message = "OK/retry response but retries exhausted; dropping the request.",
                            reason = ?reason,
                            internal_log_rate_limit = true,
                        );
                        return None;
                    }

                    warn!(message = "Retrying after response.", reason = %reason, internal_log_rate_limit = true);
                    Some(self.build_retry())
                }

                RetryAction::DontRetry(reason) => {
                    error!(message = "Not retriable; dropping the request.", reason = ?reason, internal_log_rate_limit = true);
                    None
                }

                RetryAction::Successful => None,
            },
            Err(error) => {
                if self.remaining_attempts == 0 {
                    error!(message = "Retries exhausted; dropping the request.", %error, internal_log_rate_limit = true);
                    return None;
                }

                if let Some(expected) = error.downcast_ref::<L::Error>() {
                    if self.logic.is_retriable_error(expected) {
                        warn!(message = "Retrying after error.", error = %expected, internal_log_rate_limit = true);
                        Some(self.build_retry())
                    } else {
                        error!(
                            message = "Non-retriable error; dropping the request.",
                            %error,
                            internal_log_rate_limit = true,
                        );
                        None
                    }
                } else if error.downcast_ref::<Elapsed>().is_some() {
                    warn!(
                        message = "Request timed out. If this happens often while the events are actually reaching their destination, try decreasing `batch.max_bytes` and/or using `compression` if applicable. Alternatively `request.timeout_secs` can be increased.",
                        internal_log_rate_limit = true
                    );
                    Some(self.build_retry())
                } else {
                    error!(
                        message = "Unexpected error type; dropping the request.",
                        %error,
                        internal_log_rate_limit = true
                    );
                    None
                }
            }
        }
    }

    fn clone_request(&self, request: &Req) -> Option<Req> {
        Some(request.clone())
    }
}

// Safety: `L` is never pinned and we use no unsafe pin projections
// therefore this safe.
impl<L: RetryLogic> Unpin for RetryPolicyFuture<L> {}

impl<L: RetryLogic> Future for RetryPolicyFuture<L> {
    type Output = FibonacciRetryPolicy<L>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        std::task::ready!(self.delay.poll_unpin(cx));
        Poll::Ready(self.policy.clone())
    }
}

impl RetryAction {
    pub const fn is_retryable(&self) -> bool {
        matches!(self, RetryAction::Retry(_))
    }

    pub const fn is_not_retryable(&self) -> bool {
        matches!(self, RetryAction::DontRetry(_))
    }

    pub const fn is_successful(&self) -> bool {
        matches!(self, RetryAction::Successful)
    }
}

// `tokio-retry` crate
// MIT License
// Copyright (c) 2017 Sam Rijs
//
/// A retry strategy driven by exponential back-off.
///
/// The power corresponds to the number of past attempts.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    current: u64,
    base: u64,
    factor: u64,
    max_delay: Option<Duration>,
}

impl ExponentialBackoff {
    /// Constructs a new exponential back-off strategy,
    /// given a base duration in milliseconds.
    ///
    /// The resulting duration is calculated by taking the base to the `n`-th power,
    /// where `n` denotes the number of past attempts.
    pub const fn from_millis(base: u64) -> ExponentialBackoff {
        ExponentialBackoff {
            current: base,
            base,
            factor: 1u64,
            max_delay: None,
        }
    }

    /// A multiplicative factor that will be applied to the retry delay.
    ///
    /// For example, using a factor of `1000` will make each delay in units of seconds.
    ///
    /// Default factor is `1`.
    pub const fn factor(mut self, factor: u64) -> ExponentialBackoff {
        self.factor = factor;
        self
    }

    /// Apply a maximum delay. No retry delay will be longer than this `Duration`.
    pub const fn max_delay(mut self, duration: Duration) -> ExponentialBackoff {
        self.max_delay = Some(duration);
        self
    }

    /// Resents the exponential back-off strategy to its initial state.
    pub fn reset(&mut self) {
        self.current = self.base;
    }
}

impl Iterator for ExponentialBackoff {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        // set delay duration by applying factor
        let duration = if let Some(duration) = self.current.checked_mul(self.factor) {
            Duration::from_millis(duration)
        } else {
            Duration::from_millis(std::u64::MAX)
        };

        // check if we reached max delay
        if let Some(ref max_delay) = self.max_delay {
            if duration > *max_delay {
                return Some(*max_delay);
            }
        }

        if let Some(next) = self.current.checked_mul(self.base) {
            self.current = next;
        } else {
            self.current = std::u64::MAX;
        }

        Some(duration)
    }
}

#[cfg(test)]
mod tests {
    use std::{fmt, time::Duration};

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

        policy = policy.advance();
        assert_eq!(Duration::from_secs(1), policy.backoff());

        policy = policy.advance();
        assert_eq!(Duration::from_secs(2), policy.backoff());

        policy = policy.advance();
        assert_eq!(Duration::from_secs(3), policy.backoff());

        policy = policy.advance();
        assert_eq!(Duration::from_secs(5), policy.backoff());

        policy = policy.advance();
        assert_eq!(Duration::from_secs(8), policy.backoff());

        policy = policy.advance();
        assert_eq!(Duration::from_secs(10), policy.backoff());

        policy = policy.advance();
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
                "Attempt {}: Expected backoff to be within 0 and {:?}, got {:?}",
                i + 1,
                upper_bound,
                backoff
            );

            policy = policy.advance();
        }

        // Once the max backoff is reached, it should not exceed the max backoff.
        for _ in 0..4 {
            let backoff = policy.backoff();
            assert!(
                !backoff.is_zero() && backoff <= max_duration,
                "Expected backoff to not exceed {:?}, got {:?}",
                max_duration,
                backoff
            );

            policy = policy.advance();
        }
    }

    #[derive(Debug, Clone)]
    struct SvcRetryLogic;

    impl RetryLogic for SvcRetryLogic {
        type Error = Error;
        type Response = &'static str;

        fn is_retriable_error(&self, error: &Self::Error) -> bool {
            error.0
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
