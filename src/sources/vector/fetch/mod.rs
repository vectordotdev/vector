use vector_lib::{
    EstimatedJsonEncodedSizeOf, event::Event, internal_event::InternalEventHandle, source::Source,
    source_sender::SourceSender,
};

use crate::{
    config::SourceContext,
    proto::vector as proto,
    sinks::{
        prelude::RetryLogic,
        util::{
            ServiceBuilderExt, adaptive_concurrency::AdaptiveConcurrencyLimit,
            retries::FibonacciRetryPolicy,
        },
    },
};
pub mod compression;
mod service;

fn new_client(
    tls_settings: &vector_lib::tls::MaybeTlsSettings,
    proxy_config: &vector_lib::config::proxy::ProxyConfig,
) -> crate::Result<
    hyper::Client<
        hyper_proxy::ProxyConnector<hyper_openssl::HttpsConnector<hyper::client::HttpConnector>>,
        tonic::body::BoxBody,
    >,
> {
    let proxy = crate::http::build_proxy_connector(tls_settings.clone(), proxy_config)?;

    Ok(hyper::Client::builder().http2_only(true).build(proxy))
}

pub fn with_default_scheme(address: &str, tls: bool) -> crate::Result<http::Uri> {
    let uri = address.parse::<http::Uri>()?;
    if uri.scheme().is_none() {
        // Default the scheme to http or https.
        let mut parts = uri.into_parts();

        parts.scheme = if tls {
            Some(
                "https"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("https should be valid")),
            )
        } else {
            Some(
                "http"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("http should be valid")),
            )
        };

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(
                "/".parse()
                    .unwrap_or_else(|_| unreachable!("root should be valid")),
            );
        }
        Ok(http::Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = service::PullEventsRequestError;
    type Request = proto::PullEventsRequest;
    type Response = tonic::Streaming<proto::PullEventsResponse>;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        !matches!(
            err.status.code(),
            // List taken from
            //
            // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
            tonic::Code::NotFound
                | tonic::Code::InvalidArgument
                | tonic::Code::AlreadyExists
                | tonic::Code::PermissionDenied
                | tonic::Code::OutOfRange
                | tonic::Code::Unimplemented
                | tonic::Code::Unauthenticated
                | tonic::Code::DataLoss
        )
    }
}

type TowerService = tower::limit::RateLimit<
    AdaptiveConcurrencyLimit<
        tower::retry::Retry<
            FibonacciRetryPolicy<VectorGrpcRetryLogic>,
            tower::timeout::Timeout<service::VectorService>,
        >,
        VectorGrpcRetryLogic,
    >,
>;

async fn run_pull_events_stream(
    service: &mut TowerService,
    pipeline: &mut SourceSender,
    log_namespace: &vector_lib::config::LogNamespace,
) -> Result<(), ()> {
    let mut ready_service = tower::ServiceExt::ready(service).await.unwrap();
    let mut stream = tower::Service::call(&mut ready_service, proto::PullEventsRequest {})
        .await
        .map_err(|e| {
            error!(message = "Failed to call grpc pull_events handler", %e);
            ()
        })?;
    loop {
        match stream.message().await {
            Ok(Some(response)) => {
                let mut events = response
                    .events
                    .into_iter()
                    .map(Event::from)
                    .collect::<Vec<_>>();

                let now = chrono::Utc::now();
                for event in &mut events {
                    if let Event::Log(log) = event {
                        log_namespace.insert_standard_vector_source_metadata(
                            log,
                            super::VectorConfig::NAME,
                            now,
                        );
                    }
                }

                let count = events.len();
                let byte_size = events.estimated_json_encoded_size_of();
                let events_received = register!(vector_lib::internal_event::EventsReceived);
                events_received.emit(vector_lib::internal_event::CountByteSize(count, byte_size));

                if pipeline.clone().send_batch(events).await.is_err() {
                    break;
                }
            }
            Ok(None) => {
                break;
            }
            Err(_) => {
                error!("Error reading from stream");
                break;
            }
        }
    }
    Ok(())
}

pub fn config_to_fetch_source(
    config: &super::VectorConfig,
    tls_settings: &vector_lib::tls::MaybeTlsSettings,
    cx: SourceContext,
    retry_count: usize,
) -> crate::Result<Source> {
    let client = new_client(&tls_settings, &cx.proxy)?;
    let uri = with_default_scheme(&format!("{}", config.address), tls_settings.is_tls())?;
    let service = service::VectorService::new(client, uri, config.compression);
    let mut service = tower::ServiceBuilder::new()
        .settings(config.request.into_settings(), VectorGrpcRetryLogic)
        .service(service);
    let mut log_namespace = cx.log_namespace(config.log_namespace);
    let mut shutdown = cx.shutdown;
    let mut pipeline = cx.out;
    Ok(Box::pin(async move {
        for _ in 0..retry_count {
            tokio::select! {
                _ = &mut shutdown => {
                    break;
                }
                _ = run_pull_events_stream(
                    &mut service, &mut pipeline, &mut log_namespace
                ) => {}
            }
        }
        Ok(())
    }))
}
