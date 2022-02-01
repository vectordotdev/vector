use std::{
    fs::File,
    path::Path,
    task::{Context, Poll},
};

use futures::{task::noop_waker_ref, Stream, StreamExt};

pub fn open_fixture(path: impl AsRef<Path>) -> crate::Result<serde_json::Value> {
    serde_json::from_reader(File::open(path)?).map_err(Into::into)
}

pub fn collect_ready<S>(mut rx: S) -> Vec<S::Item>
where
    S: Stream + Unpin,
{
    let waker = noop_waker_ref();
    let mut cx = Context::from_waker(waker);

    let mut vec = Vec::new();
    while let Poll::Ready(Some(item)) = rx.poll_next_unpin(&mut cx) {
        vec.push(item);
    }
    vec
}
