use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Buf;
use http_body::{Body, Frame, SizeHint};
use pin_project::pin_project;
use vector_lib::internal_event::{ByteSize, InternalEventHandle, Registered, SinkNetworkBytesSent};

/// Counts body data handed to the HTTP transport, including retries and unsuccessful responses.
///
/// This is a payload approximation: HTTP framing, headers and TLS overhead are not exposed here.
/// Do not use a size hint: a streaming body may fail or be dropped after only part of it is polled.
#[pin_project]
pub(super) struct NetworkBody<B> {
    #[pin]
    inner: B,
    bytes_sent: Registered<SinkNetworkBytesSent>,
}

impl<B> NetworkBody<B> {
    pub(super) const fn new(inner: B, bytes_sent: Registered<SinkNetworkBytesSent>) -> Self {
        Self { inner, bytes_sent }
    }
}

impl<B: Body> Body for NetworkBody<B> {
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let result = this.inner.poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &result
            && let Some(data) = frame.data_ref()
        {
            this.bytes_sent.emit(ByteSize(data.remaining()));
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use std::{io, task::Context};

    use bytes::Bytes;
    use futures::{StreamExt, stream, task::noop_waker_ref};
    use http::HeaderMap;
    use http_body_util::{BodyExt, StreamBody};
    use vector_lib::internal_event::register;

    use super::*;
    use crate::test_util::{metrics::counter_value_with_tags, trace_init};

    fn bytes_sent(protocol: &str) -> f64 {
        counter_value_with_tags(
            "component_sent_network_bytes_total",
            &[("protocol", protocol)],
        )
    }

    #[tokio::test]
    async fn network_body_counts_data_before_body_failure_and_on_retry() {
        trace_init();
        let protocol = "network_body_failure";
        let handle = register(SinkNetworkBytesSent {
            protocol: protocol.into(),
        });
        let before = bytes_sent(protocol);
        // Each attempt sends a data frame and trailers before failing. Delivery never succeeds.
        for _ in 0..2 {
            let frames = stream::iter([
                Ok(Frame::data(Bytes::from_static(b"compressed payload"))),
                Ok(Frame::trailers(HeaderMap::new())),
                Err(io::Error::other("body failed")),
            ]);
            let body = NetworkBody::new(StreamBody::new(frames), handle.clone());
            assert!(body.collect().await.is_err());
        }
        assert_eq!(
            (bytes_sent(protocol) - before).to_bits(),
            36.0_f64.to_bits()
        );
        assert_eq!(
            counter_value_with_tags("component_sent_bytes_total", &[("protocol", protocol)])
                .to_bits(),
            0.0_f64.to_bits(),
            "network accounting must not change the serialized-byte counter",
        );
    }

    #[test]
    fn network_body_does_not_count_unpolled_or_pending_data() {
        trace_init();
        let protocol = "network_body_pending";
        let handle = register(SinkNetworkBytesSent {
            protocol: protocol.into(),
        });
        let before = bytes_sent(protocol);
        let frames =
            stream::pending::<Result<Frame<Bytes>, io::Error>>().chain(stream::iter([Ok(
                Frame::data(Bytes::from_static(b"unsent")),
            )]));
        let mut body = Box::pin(NetworkBody::new(StreamBody::new(frames), handle));
        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(body.as_mut().poll_frame(&mut cx).is_pending());
        drop(body);
        assert_eq!(bytes_sent(protocol).to_bits(), before.to_bits());
    }
}
