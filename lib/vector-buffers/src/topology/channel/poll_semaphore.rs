use futures::ready;
use std::fmt;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio_util::sync::ReusableBoxFuture;

// NOTE: `PollSemaphore` has been directly vendored here via copy/paste due to issues with
// overriding a single crate like `tokio-util` and having it spiral out in a way that causes issues
// like "perhaps two different versions of crate `tokio` are being used?".

/// A wrapper around [`Semaphore`] that provides a "poll"-friendly methods for acquiring permits.
///
/// [`Semaphore`]: tokio::sync::Semaphore
pub struct PollSemaphore {
    semaphore: Arc<Semaphore>,
    permit_fut: Option<ReusableBoxFuture<'static, Result<OwnedSemaphorePermit, AcquireError>>>,
}

impl PollSemaphore {
    /// Create a new `PollSemaphore`.
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        Self {
            semaphore,
            permit_fut: None,
        }
    }

    /// Closes the semaphore.
    pub fn close(&self) {
        self.semaphore.close();
    }

    /// Whether or not the underlying semaphore is closed.
    pub fn is_closed(&self) -> bool {
        self.semaphore.is_closed()
    }

    /// Returns the current number of available permits.
    ///
    /// This is equivalent to the [`Semaphore::available_permits`] method on the
    /// `tokio::sync::Semaphore` type.
    ///
    /// [`Semaphore::available_permits`]: tokio::sync::Semaphore::available_permits
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Poll to acquire `n` permits from the semaphore.
    ///
    /// This can return the following values:
    ///
    ///  - `Poll::Pending` if the permits are not currently available.
    ///  - `Poll::Ready(Some(permit))` if the permits were acquired.
    ///  - `Poll::Ready(None)` if the semaphore has been closed.
    ///
    /// When this method returns `Poll::Pending`, the current task is scheduled
    /// to receive a wakeup when the permits becomes available, or when the
    /// semaphore is closed. Note that on multiple calls to `poll_acquire`, only
    /// the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup.
    pub fn poll_acquire_many(
        &mut self,
        n: u32,
        cx: &mut Context<'_>,
    ) -> Poll<Option<OwnedSemaphorePermit>> {
        let permit_future = if let Some(fut) = self.permit_fut.as_mut() {
            fut
        } else {
            // Avoid allocations completely if we can grab a permit immediately.
            match Arc::clone(&self.semaphore).try_acquire_many_owned(n) {
                Ok(permit) => return Poll::Ready(Some(permit)),
                Err(TryAcquireError::Closed) => return Poll::Ready(None),
                Err(TryAcquireError::NoPermits) => {}
            }

            let next_fut = Arc::clone(&self.semaphore).acquire_many_owned(n);
            self.permit_fut
                .get_or_insert(ReusableBoxFuture::new(next_fut))
        };

        let result = ready!(permit_future.poll(cx));

        let next_fut = Arc::clone(&self.semaphore).acquire_many_owned(n);
        permit_future.set(next_fut);

        match result {
            Ok(permit) => Poll::Ready(Some(permit)),
            Err(_closed) => {
                self.permit_fut = None;
                Poll::Ready(None)
            }
        }
    }
}

impl Clone for PollSemaphore {
    fn clone(&self) -> PollSemaphore {
        Self {
            semaphore: Arc::clone(&self.semaphore),
            permit_fut: None,
        }
    }
}

impl fmt::Debug for PollSemaphore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollSemaphore")
            .field("semaphore", &self.semaphore)
            .finish()
    }
}

impl AsRef<Semaphore> for PollSemaphore {
    fn as_ref(&self) -> &Semaphore {
        &*self.semaphore
    }
}
