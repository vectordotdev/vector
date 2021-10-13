use crate::buffers::Ackable;

use crate::event::{EventFinalizers, Finalizable, EventStatus};
use hyper::service::Service;
use std::task::{Context, Poll};
use crate::http::HttpClient;
use futures::future::BoxFuture;
use hyper::{Body, Request};
use futures::FutureExt;


pub struct ElasticSearchRequest {
    pub http_request: Request<Vec<u8>>,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
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
    pub http_client: HttpClient
}

pub struct ElasticSearchResponse {

}

impl AsRef<EventStatus> for ElasticSearchResponse {
    fn as_ref(&self) -> &EventStatus {
        //TODO: use the correct status
        &EventStatus::Delivered
    }
}

impl Service<ElasticSearchRequest> for ElasticSearchService {
    type Response = ElasticSearchResponse;
    type Error = ();
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ElasticSearchRequest) -> Self::Future {
        let http_req = req.http_request.map(Body::from);
        let mut http_client = self.http_client.clone();
        Box::pin(async move {
            http_client.call(http_req).await;
            Ok(ElasticSearchResponse{})
        })
    }
}
