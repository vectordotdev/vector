use super::TowerError;
use futures::{try_ready, Async, Future, Poll};
use log::{error, warn};
use std::{
    error::Error as StdError,
    time::{Duration, Instant},
};
use tokio::timer::Delay;
use tower_retry::Policy;

pub trait RetryLogic: Clone {
    type Error: StdError + 'static;
    type Response;

    fn is_retriable_error(&self, error: &Self::Error) -> bool;

    fn should_retry_response(&self, _response: &Self::Response) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct FixedRetryPolicy<Logic: RetryLogic> {
    remaining_attempts: usize,
    backoff: Duration,
    logic: Logic,
}

pub struct RetryPolicyFuture<Logic: RetryLogic> {
    delay: Delay,
    policy: FixedRetryPolicy<Logic>,
}

impl<Logic: RetryLogic> FixedRetryPolicy<Logic> {
    pub fn new(remaining_attempts: usize, backoff: Duration, logic: Logic) -> Self {
        FixedRetryPolicy {
            remaining_attempts,
            backoff,
            logic,
        }
    }

    fn build_retry(&self) -> RetryPolicyFuture<Logic> {
        let policy = FixedRetryPolicy::new(
            self.remaining_attempts - 1,
            self.backoff.clone(),
            self.logic.clone(),
        );
        let next = Instant::now() + self.backoff;
        let delay = Delay::new(next);

        RetryPolicyFuture { delay, policy }
    }
}

impl<Req, Res, Logic> Policy<Req, Res, TowerError> for FixedRetryPolicy<Logic>
where
    Req: Clone,
    Logic: RetryLogic<Response = Res>,
{
    type Future = RetryPolicyFuture<Logic>;

    fn retry(&self, _: &Req, result: Result<&Res, &TowerError>) -> Option<Self::Future> {
        match result {
            Ok(response) => {
                if self.logic.should_retry_response(response) {
                    warn!("retrying after response");
                    Some(self.build_retry())
                } else {
                    None
                }
            }
            Err(error) => {
                if self.remaining_attempts == 0 {
                    error!("retries exhausted: {}", error);
                    return None;
                }

                if let Some(expected) = error.downcast_ref() {
                    if self.logic.is_retriable_error(expected) {
                        warn!("retrying after error: {}", expected);
                        Some(self.build_retry())
                    } else {
                        error!("encountered non-retriable error: {}", error);
                        None
                    }
                } else {
                    warn!("unexpected error type: {}", error);
                    None
                }
            }
        }
    }

    fn clone_request(&self, request: &Req) -> Option<Req> {
        Some(request.clone())
    }
}

impl<Logic: RetryLogic> Future for RetryPolicyFuture<Logic> {
    type Item = FixedRetryPolicy<Logic>;
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self.delay.poll().map_err(|_| ()));
        Ok(Async::Ready(self.policy.clone()))
    }
}
