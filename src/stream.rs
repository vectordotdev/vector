use std::task::Poll;

pub(crate) async fn tripwire_handler(closed: bool) {
    futures::future::poll_fn(|_| {
        if closed {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await
}
