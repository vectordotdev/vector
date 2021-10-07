use crate::buffers::Ackable;
use bytes::Bytes;
use crate::event::{EventFinalizers, Finalizable};
use hyper::service::Service;
use std::task::{Context, Poll};
use crate::http::HttpClient;
use crate::http;
use futures::future::BoxFuture;
use hyper::Body;
use futures::FutureExt;
use tracing::Instrument;

pub struct ElasticSearchRequest {
    http_request: http::Request<Vec<u8>>
}


impl Ackable for ElasticSearchRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for ElasticSearchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

pub struct ElasticSearchService {
    http_client: HttpClient
}

impl Service<ElasticSearchRequest> for ElasticSearchService {
    type Response = ();
    type Error = ();
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ElasticSearchRequest) -> Self::Future {
        let http_req = req.http_request.map(Body::from);
        Box::pin(async move {
            self.http_client.call(http_req).await;
            Ok(())
        })
    }
}
