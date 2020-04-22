use crate::Error;
use futures::FutureExt;
use std::{
    cmp,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::{delay_for, Delay};
use tower03::{retry::Policy, timeout::error::Elapsed};

pub enum RetryAction {
    /// Indicate that this request should be retried with a reason
    Retry(String),
    /// Indicate that this request should not be retried with a reason
    DontRetry(String),
    /// Indicate that this request should not be retried but the request was successful
    Successful,
}

pub trait RetryLogic: Clone {
    type Error: std::error::Error + Send + Sync + 'static;
    type Response;

    fn is_retriable_error(&self, error: &Self::Error) -> bool;

    fn should_retry_response(&self, _response: &Self::Response) -> RetryAction {
        // Treat the default as the request is successful
        RetryAction::Successful
    }
}

#[derive(Debug, Clone)]
pub struct FixedRetryPolicy<L> {
    remaining_attempts: usize,
    previous_duration: Duration,
    current_duration: Duration,
    max_duration: Duration,
    logic: L,
}

pub struct RetryPolicyFuture<L: RetryLogic> {
    delay: Delay,
    policy: FixedRetryPolicy<L>,
}

impl<L: RetryLogic> FixedRetryPolicy<L> {
    pub fn new(
        remaining_attempts: usize,
        initial_backoff: Duration,
        max_duration: Duration,
        logic: L,
    ) -> Self {
        FixedRetryPolicy {
            remaining_attempts,
            previous_duration: Duration::from_secs(0),
            current_duration: initial_backoff,
            max_duration,
            logic,
        }
    }

    fn advance(&self) -> FixedRetryPolicy<L> {
        let next_duration: Duration = self.previous_duration + self.current_duration;

        FixedRetryPolicy {
            remaining_attempts: self.remaining_attempts - 1,
            previous_duration: self.current_duration,
            current_duration: cmp::min(next_duration, self.max_duration),
            max_duration: self.max_duration,
            logic: self.logic.clone(),
        }
    }

    fn backoff(&self) -> Duration {
        self.current_duration
    }

    fn build_retry(&self) -> RetryPolicyFuture<L> {
        let policy = self.advance();
        let delay = delay_for(self.backoff());

        debug!(message = "retrying request.", delay_ms = %self.backoff().as_millis());
        RetryPolicyFuture { delay, policy }
    }
}

impl<Req, Res, L> Policy<Req, Res, Error> for FixedRetryPolicy<L>
where
    Req: Clone,
    L: RetryLogic<Response = Res>,
{
    type Future = RetryPolicyFuture<L>;

    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        match result {
            Ok(response) => {
                if self.remaining_attempts == 0 {
                    error!("retries exhausted");
                    return None;
                }

                match self.logic.should_retry_response(response) {
                    RetryAction::Retry(reason) => {
                        warn!(message = "retrying after response.", %reason);
                        Some(self.build_retry())
                    }

                    RetryAction::DontRetry(reason) => {
                        warn!(message = "request is not retryable; dropping the request.", %reason);
                        None
                    }

                    RetryAction::Successful => None,
                }
            }
            Err(error) => {
                if self.remaining_attempts == 0 {
                    error!(message = "retries exhausted.", %error);
                    return None;
                }

                if let Some(expected) = error.downcast_ref::<L::Error>() {
                    if self.logic.is_retriable_error(expected) {
                        warn!("retrying after error: {}", expected);
                        Some(self.build_retry())
                    } else {
                        error!(message = "encountered non-retriable error.", %error);
                        None
                    }
                } else if error.downcast_ref::<Elapsed>().is_some() {
                    warn!("request timedout.");
                    Some(self.build_retry())
                } else {
                    warn!(message = "unexpected error type.", %error);
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
    type Output = FixedRetryPolicy<L>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        futures::ready!(self.delay.poll_unpin(cx));
        Poll::Ready(self.policy.clone())
    }
}

impl RetryAction {
    pub fn is_retryable(&self) -> bool {
        if let RetryAction::Retry(_) = &self {
            true
        } else {
            false
        }
    }

    pub fn is_not_retryable(&self) -> bool {
        if let RetryAction::DontRetry(_) = &self {
            true
        } else {
            false
        }
    }

    pub fn is_successful(&self) -> bool {
        if let RetryAction::Successful = &self {
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::trace_init;
    use std::{fmt, time::Duration};
    use tokio::time;
    use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok, task};
    use tower03::retry::RetryLayer;
    use tower_test03::{assert_request_eq, mock};

    #[tokio::test]
    async fn service_error_retry() {
        time::pause();
        trace_init();

        let policy = FixedRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
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

        let policy = FixedRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let mut fut = task::spawn(svc.call("hello"));
        assert_request_eq!(handle, "hello").send_error(Error(false));
        assert_ready_err!(fut.poll());
    }

    #[tokio::test]
    async fn timeout_error() {
        time::pause();
        trace_init();

        let policy = FixedRetryPolicy::new(
            5,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
        );

        let (mut svc, mut handle) = mock::spawn_layer(RetryLayer::new(policy));

        assert_ready_ok!(svc.poll_ready());

        let mut fut = task::spawn(svc.call("hello"));
        assert_request_eq!(handle, "hello").send_error(tower03::timeout::error::Elapsed::new());
        assert_pending!(fut.poll());

        time::advance(Duration::from_secs(2)).await;
        assert_pending!(fut.poll());

        assert_request_eq!(handle, "hello").send_response("world");
        assert_eq!(fut.await.unwrap(), "world");
    }

    #[test]
    fn backoff_grows_to_max() {
        let mut policy = FixedRetryPolicy::new(
            10,
            Duration::from_secs(1),
            Duration::from_secs(10),
            SvcRetryLogic,
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
