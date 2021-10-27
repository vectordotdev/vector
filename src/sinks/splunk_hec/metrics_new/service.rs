use futures_util::future::BoxFuture;
use tower::Service;

use crate::sinks::splunk_hec::common::response::HecResponse;

use super::request_builder::HecMetricsRequest;

pub struct HecMetricsService {

}

impl Service<HecMetricsRequest> for HecMetricsService {
    type Response = HecResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn call(&mut self, req: HecMetricsRequest) -> Self::Future {
        todo!()
    }
}

