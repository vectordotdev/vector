use std::{
    fmt,
    future::Future,
    mem,
    sync::Arc,
    task::{ready, Context, Poll},
};

use futures::future::BoxFuture;
use tokio::sync::OwnedSemaphorePermit;
use tower::{load::Load, Service};

use super::{controller::Controller, future::ResponseFuture, AdaptiveConcurrencySettings};
use crate::sinks::util::retries::RetryLogic;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle. Automatically expands and contracts the actual
/// concurrency limit depending on observed request response behavior.
pub struct AdaptiveConcurrencyLimit<S, L> {
    inner: S,
    pub(super) controller: Arc<Controller<L>>,
    state: State,
}

enum State {
    Waiting(BoxFuture<'static, OwnedSemaphorePermit>),
    Ready(OwnedSemaphorePermit),
    Empty,
}

impl<S, L> AdaptiveConcurrencyLimit<S, L> {
    /// Create a new automated concurrency limiter.
    pub(crate) fn new(
        inner: S,
        logic: L,
        concurrency: Option<usize>,
        options: AdaptiveConcurrencySettings,
    ) -> Self {
        AdaptiveConcurrencyLimit {
            inner,
            controller: Arc::new(Controller::new(concurrency, options, logic)),
            state: State::Empty,
        }
    }
}

impl<S, L, Request> Service<Request> for AdaptiveConcurrencyLimit<S, L>
where
    S: Service<Request>,
    S::Error: Into<crate::Error>,
    L: RetryLogic<Response = S::Response>,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = ResponseFuture<S::Future, L>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                State::Ready(_) => return self.inner.poll_ready(cx).map_err(Into::into),
                State::Waiting(ref mut fut) => {
                    tokio::pin!(fut);
                    let permit = ready!(fut.poll(cx));
                    State::Ready(permit)
                }
                State::Empty => State::Waiting(Box::pin(Arc::clone(&self.controller).acquire())),
            };
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Make sure a permit has been acquired
        let permit = match mem::replace(&mut self.state, State::Empty) {
            // Take the permit.
            State::Ready(permit) => permit,
            // whoopsie!
            _ => panic!("Maximum requests in-flight; poll_ready must be called first"),
        };

        self.controller.start_request();

        // Call the inner service
        let future = self.inner.call(request);

        ResponseFuture::new(future, permit, Arc::clone(&self.controller))
    }
}

impl<S, L> Load for AdaptiveConcurrencyLimit<S, L> {
    type Metric = f64;

    fn load(&self) -> Self::Metric {
        self.controller.load()
    }
}

impl<S, L> Clone for AdaptiveConcurrencyLimit<S, L>
where
    S: Clone,
    L: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            controller: Arc::clone(&self.controller),
            state: State::Empty,
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Waiting(_) => f
                .debug_tuple("State::Waiting")
                .field(&format_args!("..."))
                .finish(),
            State::Ready(ref r) => f.debug_tuple("State::Ready").field(&r).finish(),
            State::Empty => f.debug_tuple("State::Empty").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Mutex, MutexGuard},
        time::Duration,
    };

    use snafu::Snafu;
    use tokio::time::{advance, pause};
    use tokio_test::{assert_pending, assert_ready_ok};
    use tower_test::{
        assert_request_eq,
        mock::{
            self, future::ResponseFuture as MockResponseFuture, Handle, Mock, SendResponse, Spawn,
        },
    };

    use super::{
        super::{
            controller::{ControllerStatistics, Inner},
            AdaptiveConcurrencyLimitLayer,
        },
        *,
    };
    use crate::assert_downcast_matches;

    #[derive(Clone, Copy, Debug, Snafu)]
    enum TestError {
        Deferral,
    }

    #[derive(Clone, Copy, Debug)]
    struct TestRetryLogic;
    impl RetryLogic for TestRetryLogic {
        type Error = TestError;
        type Response = String;
        fn is_retriable_error(&self, _error: &Self::Error) -> bool {
            true
        }
    }

    type TestInner = AdaptiveConcurrencyLimit<Mock<String, String>, TestRetryLogic>;
    struct TestService {
        service: Spawn<TestInner>,
        handle: Handle<String, String>,
        inner: Arc<Mutex<Inner>>,
        stats: Arc<Mutex<ControllerStatistics>>,
        sequence: usize,
    }

    struct Send {
        request: ResponseFuture<MockResponseFuture<String>, TestRetryLogic>,
        response: SendResponse<String>,
        sequence: usize,
    }

    impl TestService {
        fn start() -> Self {
            let layer = AdaptiveConcurrencyLimitLayer::new(
                None,
                AdaptiveConcurrencySettings {
                    decrease_ratio: 0.5,
                    ..Default::default()
                },
                TestRetryLogic,
            );
            let (service, handle) = mock::spawn_layer(layer);
            let controller = Arc::clone(&service.get_ref().controller);
            let inner = Arc::clone(&controller.inner);
            let stats = Arc::clone(&controller.stats);
            Self {
                service,
                handle,
                inner,
                stats,
                sequence: 0,
            }
        }

        async fn run<F, Ret>(doit: F) -> ControllerStatistics
        where
            F: FnOnce(Self) -> Ret,
            Ret: Future<Output = ()>,
        {
            let svc = Self::start();
            //let inner = svc.inner.clone();
            let stats = Arc::clone(&svc.stats);
            pause();
            doit(svc).await;
            //dbg!(inner);
            Arc::try_unwrap(stats).unwrap().into_inner().unwrap()
        }

        async fn send(&mut self, is_ready: bool) -> Send {
            assert_ready_ok!(self.service.poll_ready());
            self.sequence += 1;
            let data = format!("REQUEST #{}", self.sequence);
            let request = self.service.call(data.clone());
            let response = assert_request_eq!(self.handle, data);
            if is_ready {
                assert_ready_ok!(self.service.poll_ready());
            } else {
                assert_pending!(self.service.poll_ready());
            }
            Send {
                request,
                response,
                sequence: self.sequence,
            }
        }

        fn inner(&self) -> MutexGuard<Inner> {
            self.inner.lock().unwrap()
        }
    }

    impl Send {
        async fn respond(self) {
            let data = format!("RESPONSE #{}", self.sequence);
            self.response.send_response(data.clone());
            assert_eq!(self.request.await.unwrap(), data);
        }

        async fn defer(self) {
            self.response.send_error(TestError::Deferral);
            assert_downcast_matches!(
                self.request.await.unwrap_err(),
                TestError,
                TestError::Deferral
            );
        }
    }

    #[tokio::test]
    async fn startup_conditions() {
        TestService::run(|mut svc| async move {
            // Concurrency starts at 1
            assert_eq!(svc.inner().current_limit, 1);
            svc.send(false).await;
        })
        .await;
    }

    #[tokio::test]
    async fn increases_limit() {
        let stats = TestService::run(|mut svc| async move {
            // Concurrency starts at 1
            assert_eq!(svc.inner().current_limit, 1);
            let req = svc.send(false).await;
            advance(Duration::from_secs(1)).await;
            req.respond().await;

            // Concurrency stays at 1 until a measurement
            assert_eq!(svc.inner().current_limit, 1);
            let req = svc.send(false).await;
            advance(Duration::from_secs(1)).await;
            req.respond().await;

            // After a constant speed measurement, concurrency is increased
            assert_eq!(svc.inner().current_limit, 2);
        })
        .await;

        let in_flight = stats.in_flight.stats().unwrap();
        assert_eq!(in_flight.max, 1);
        assert_eq!(in_flight.mean, 1.0);

        let observed_rtt = stats.observed_rtt.stats().unwrap();
        assert_eq!(observed_rtt.mean, 1.0);
    }

    #[tokio::test]
    async fn handles_deferral() {
        TestService::run(|mut svc| async move {
            assert_eq!(svc.inner().current_limit, 1);
            let req = svc.send(false).await;
            advance(Duration::from_secs(1)).await;
            req.respond().await;

            assert_eq!(svc.inner().current_limit, 1);
            let req = svc.send(false).await;
            advance(Duration::from_secs(1)).await;
            req.respond().await;

            assert_eq!(svc.inner().current_limit, 2);

            let req = svc.send(true).await;
            advance(Duration::from_secs(1)).await;
            req.defer().await;
            assert_eq!(svc.inner().current_limit, 1);
        })
        .await;
    }

    #[tokio::test]
    async fn rapid_decrease() {
        TestService::run(|mut svc| async move {
            let mut reqs = [None, None, None];
            for &concurrent in &[1, 1, 2, 3] {
                assert_eq!(svc.inner().current_limit, concurrent);
                // This would ideally be done with something like:
                // let reqs = futures::future::join_all((0..concurrent).map(svc.send)).await
                // but that runs afoul of the borrow checker since `svc`
                // must be borrowed mutable with a non-static
                // lifetime. Resolving it is more work than it's worth
                // for this test.
                for (i, req) in reqs.iter_mut().take(concurrent).enumerate() {
                    *req = Some(svc.send(i < concurrent - 1).await);
                }
                advance(Duration::from_secs(1)).await;
                for req in reqs.iter_mut().take(concurrent) {
                    req.take().unwrap().respond().await;
                }
            }

            assert_eq!(svc.inner().current_limit, 4);

            let req = svc.send(true).await;
            advance(Duration::from_secs(1)).await;
            req.defer().await;

            assert_eq!(svc.inner().current_limit, 2);
        })
        .await;
    }
}
