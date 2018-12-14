use futures::{try_ready, Async, AsyncSink, Future, Sink};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request,
};
use hyper_tls::HttpsConnector;
use log::error;
use std::mem;
use tokio::executor::DefaultExecutor;

// TODO: test this against a fake server
pub struct HttpSink {
    client: Client<HttpsConnector<HttpConnector>, Body>,
    in_flight_request: Option<ResponseFuture>,
}

impl HttpSink {
    pub fn new() -> Self {
        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        Self {
            client,
            in_flight_request: None,
        }
    }
}

impl Sink for HttpSink {
    type SinkItem = Request<Body>;
    type SinkError = ();

    fn start_send(
        &mut self,
        request: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.in_flight_request.is_some() {
            self.poll_complete()?;
            if self.in_flight_request.is_some() {
                return Ok(AsyncSink::NotReady(request));
            }
        }

        let request = self.client.request(request);

        self.in_flight_request = Some(request);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            if let Some(ref mut in_flight_request) = self.in_flight_request {
                let _response =
                    try_ready!(in_flight_request.poll().map_err(|e| error!("err: {}", e)));

                // TODO: retry on errors

                self.in_flight_request = None;
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }
}

pub struct SizeBuffered<S: Sink<SinkItem = Vec<u8>>> {
    inner: S,
    buffer: Vec<u8>,
    buffer_limit: usize,
}

impl<S: Sink<SinkItem = Vec<u8>>> SizeBuffered<S> {
    pub fn new(inner: S, limit: usize) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            buffer_limit: limit,
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Sink<SinkItem = Vec<u8>>> Sink for SizeBuffered<S> {
    type SinkItem = Vec<u8>;
    type SinkError = S::SinkError;

    fn start_send(
        &mut self,
        mut item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.buffer.len() >= self.buffer_limit {
            self.poll_complete()?;

            if self.buffer.len() >= self.buffer_limit {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.buffer.append(&mut item);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            try_ready!(self.inner.poll_complete());

            if self.buffer.is_empty() {
                return Ok(Async::Ready(()));
            } else {
                let buffer = mem::replace(&mut self.buffer, Vec::new());
                match self.inner.start_send(buffer)? {
                    AsyncSink::Ready => {}
                    AsyncSink::NotReady(_item) => {
                        unreachable!("Will only get here if inner.poll_complete() returned Ready")
                    }
                }
            }
        }
    }
}

pub trait SinkExt: Sink<SinkItem = Vec<u8>> + Sized {
    fn size_buffered(self, limit: usize) -> SizeBuffered<Self> {
        SizeBuffered::new(self, limit)
    }
}

impl<S> SinkExt for S where S: Sink<SinkItem = Vec<u8>> + Sized {}

#[cfg(test)]
mod test {
    use super::SizeBuffered;
    use futures::{Future, Sink};

    #[test]
    fn size_buffered_buffers_messages_until_limit() {
        let buffered = SizeBuffered::new(vec![], 10);

        let input = (0..22).map(|i| vec![i]).collect::<Vec<_>>();
        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered.into_inner();
        assert_eq!(
            output,
            vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
                vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
                vec![20, 21]
            ]
        );
    }

    #[test]
    fn size_buffered_doesnt_buffer_if_its_flushed() {
        let buffered = SizeBuffered::new(vec![], 10);

        let buffered = buffered.send(vec![0]).wait().unwrap();
        let buffered = buffered.send(vec![1]).wait().unwrap();

        let output = buffered.into_inner();
        assert_eq!(output, vec![vec![0], vec![1],]);
    }

    #[test]
    fn size_buffered_allows_the_final_item_to_exceed_the_buffer_size() {
        let buffered = SizeBuffered::new(vec![], 10);

        let input = vec![
            vec![0, 1, 2],
            vec![3, 4, 5],
            vec![6, 7, 8],
            vec![9, 10, 11],
            vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
            vec![24],
        ];
        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered.into_inner();
        assert_eq!(
            output,
            vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
                vec![24],
            ]
        );
    }
}
