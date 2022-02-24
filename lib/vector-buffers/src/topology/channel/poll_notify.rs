use futures::ready;
use std::fmt;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Notify;
use tokio_util::sync::ReusableBoxFuture;

/// A wrapper around [`Notify`] that provides a "poll"-friendly methods for waiting for notifications.
///
/// [`Notify`]: tokio::sync::Notify
pub struct PollNotify {
    notify: Arc<Notify>,
    notified_fut: Option<ReusableBoxFuture<'static, Arc<Notify>>>,
}

impl PollNotify {
    /// Create a new `PollNotify`.
    pub fn new(notify: Arc<Notify>) -> Self {
        Self {
            notify,
            notified_fut: None,
        }
    }

    /// Poll to be notified.
    ///
    /// This can return the following values:
    ///
    ///  - `Poll::Pending` if there was no stored notification.
    ///  - `Poll::Ready(())` if a notification was received.
    ///
    /// When this method returns `Poll::Pending`, the current task is scheduled to receive a wakeup
    /// when a notification is triggered. Note that on multiple calls to `poll_notify`, only the
    /// `Waker` from the `Context` passed to the most recent call is scheduled to receive a wakeup.
    pub fn poll_notify(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        // Get or create a future for calling `Notify::notified` and awaiting it.
        let notified_future = if let Some(fut) = self.notified_fut.as_mut() {
            fut
        } else {
            let notify = Arc::clone(&self.notify);
            let fut = async move {
                notify.notified().await;
                notify
            };
            self.notified_fut.get_or_insert(ReusableBoxFuture::new(fut))
        };

        // Re-arm our future, saving the allocation held by `ReusableBoxFuture`.
        let notify = ready!(notified_future.poll(cx));
        let fut = async move {
            notify.notified().await;
            notify
        };
        notified_future.set(fut);

        Poll::Ready(())
    }
}

impl Clone for PollNotify {
    fn clone(&self) -> PollNotify {
        Self {
            notify: Arc::clone(&self.notify),
            notified_fut: None,
        }
    }
}

impl fmt::Debug for PollNotify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollNotify")
            .field("notify", &self.notify)
            .finish()
    }
}

impl AsRef<Notify> for PollNotify {
    fn as_ref(&self) -> &Notify {
        &*self.notify
    }
}
