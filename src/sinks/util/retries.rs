use super::Error;
use futures::{try_ready, Async, Future, Poll};
use std::{
    borrow::Cow,
    cmp,
    time::{Duration, Instant},
};
use tokio::timer::Delay;
use tower::{retry::Policy, timeout::error::Elapsed};

const MAX_BACKOFF: Duration = Duration::from_secs(10);

pub trait RetryLogic: Clone {
    type Error: std::error::Error + Send + Sync + 'static;
    type Response;

    fn is_retriable_error(&self, error: &Self::Error) -> bool;

    fn should_retry_response(&self, _response: &Self::Response) -> Option<Cow<str>> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct FixedRetryPolicy<L: RetryLogic> {
    remaining_attempts: usize,
    durations: Vec<Duration>,
    logic: L,
}

pub struct RetryPolicyFuture<L: RetryLogic> {
    delay: Delay,
    policy: FixedRetryPolicy<L>,
}

impl<L: RetryLogic> FixedRetryPolicy<L> {
    pub fn new(remaining_attempts: usize, backoff: Duration, logic: L) -> Self {
        FixedRetryPolicy {
            remaining_attempts,
            durations: vec![Duration::from_secs(0), backoff],
            logic,
        }
    }

    fn advance(&self) -> FixedRetryPolicy<L> {
        debug_assert!(self.durations.len() == 2);
        let next_duration: Duration = self.durations.iter().sum();

        FixedRetryPolicy {
            remaining_attempts: self.remaining_attempts - 1,
            durations: vec![self.durations[1], cmp::min(next_duration, MAX_BACKOFF)],
            logic: self.logic.clone(),
        }
    }

    fn backoff(&self) -> Duration {
        debug_assert!(self.durations.len() == 2);
        self.durations[1]
    }

    fn build_retry(&self) -> RetryPolicyFuture<L> {
        let policy = self.advance();
        let next = Instant::now() + policy.backoff();
        let delay = Delay::new(next);

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

                if let Some(ref reason) = self.logic.should_retry_response(response) {
                    warn!(message = "retrying after response.", %reason);
                    Some(self.build_retry())
                } else {
                    None
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
                } else if let Some(_) = error.downcast_ref::<Elapsed>() {
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

impl<L: RetryLogic> Future for RetryPolicyFuture<L> {
    type Item = FixedRetryPolicy<L>;
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self
            .delay
            .poll()
            .map_err(|error| panic!("timer error: {}; this is a bug!", error)));
        Ok(Async::Ready(self.policy.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::trace_init;
    use futures::Future;
    use std::{fmt, time::Duration};
    use tokio01_test::{assert_err, assert_not_ready, assert_ready, clock};
    use tower::{retry::Retry, Service};
    use tower_test::{assert_request_eq, mock};

    #[test]
    fn service_error_retry() {
        clock::mock(|clock| {
            trace_init();

            let policy = FixedRetryPolicy::new(5, Duration::from_secs(1), SvcRetryLogic);

            let (service, mut handle) = mock::pair();
            let mut svc = Retry::new(policy, service);

            assert_ready!(svc.poll_ready());

            let mut fut = svc.call("hello");
            assert_request_eq!(handle, "hello").send_error(Error(true));
            assert_not_ready!(fut.poll());

            clock.advance(Duration::from_secs(2));
            assert_not_ready!(fut.poll());

            assert_request_eq!(handle, "hello").send_response("world");
            assert_eq!(fut.wait().unwrap(), "world");
        });
    }

    #[test]
    fn service_error_no_retry() {
        trace_init();

        let policy = FixedRetryPolicy::new(5, Duration::from_secs(1), SvcRetryLogic);

        let (service, mut handle) = mock::pair();
        let mut svc = Retry::new(policy, service);

        assert_ready!(svc.poll_ready());

        let mut fut = svc.call("hello");
        assert_request_eq!(handle, "hello").send_error(Error(false));
        assert_err!(fut.poll());
    }

    #[test]
    fn timeout_error() {
        clock::mock(|clock| {
            trace_init();

            let policy = FixedRetryPolicy::new(5, Duration::from_secs(1), SvcRetryLogic);

            let (service, mut handle) = mock::pair();
            let mut svc = Retry::new(policy, service);

            assert_ready!(svc.poll_ready());

            let mut fut = svc.call("hello");
            assert_request_eq!(handle, "hello").send_error(tower::timeout::error::Elapsed::new());
            assert_not_ready!(fut.poll());

            clock.advance(Duration::from_secs(2));
            assert_not_ready!(fut.poll());

            assert_request_eq!(handle, "hello").send_response("world");
            assert_eq!(fut.wait().unwrap(), "world");
        });
    }

    #[test]
    fn backoff_grows_to_max() {
        let mut policy = FixedRetryPolicy::new(10, Duration::from_secs(1), SvcRetryLogic);
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
