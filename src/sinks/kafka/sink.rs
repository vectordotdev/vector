use std::time::Duration;

use rdkafka::{
    ClientConfig,
    error::KafkaError,
    producer::{BaseProducer, FutureProducer, Producer},
};
use snafu::{ResultExt, Snafu};
use tower::limit::RateLimit;
use tracing::Span;
use vrl::path::OwnedTargetPath;

use super::config::KafkaSinkConfig;
use crate::{
    config::SinkHealthcheckOptions,
    kafka::KafkaStatisticsContext,
    sinks::{
        kafka::{request_builder::KafkaRequestBuilder, service::KafkaService},
        prelude::*,
    },
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(super) enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: KafkaError },
}

pub struct KafkaSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    service: RateLimit<KafkaService>,
    topic: Template,
    key_field: Option<OwnedTargetPath>,
    headers_key: Option<OwnedTargetPath>,
}

pub(crate) fn create_producer(
    client_config: ClientConfig,
    #[cfg(feature = "aws-core")] msk_iam: Option<crate::kafka::MskIamCredentials>,
) -> crate::Result<FutureProducer<KafkaStatisticsContext>> {
    #[cfg(feature = "aws-core")]
    let context = KafkaStatisticsContext::new(false, Span::current(), msk_iam);
    #[cfg(not(feature = "aws-core"))]
    let context = KafkaStatisticsContext::new(false, Span::current());
    let producer = client_config
        .create_with_context(context)
        .context(KafkaCreateFailedSnafu)?;
    Ok(producer)
}

impl KafkaSink {
    pub(crate) fn new(
        config: KafkaSinkConfig,
        #[cfg(feature = "aws-core")] msk_iam: Option<crate::kafka::MskIamCredentials>,
    ) -> crate::Result<Self> {
        let producer_config = config.to_rdkafka()?;
        let producer = create_producer(
            producer_config,
            #[cfg(feature = "aws-core")]
            msk_iam,
        )?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(KafkaSink {
            headers_key: config.headers_key.map(|key| key.0),
            transformer,
            encoder,
            service: ServiceBuilder::new()
                .rate_limit(
                    config.rate_limit_num,
                    Duration::from_secs(config.rate_limit_duration_secs),
                )
                .service(KafkaService::new(producer)),
            topic: config.topic,
            key_field: config.key_field.map(|key| key.0),
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = KafkaRequestBuilder {
            key_field: self.key_field,
            headers_key: self.headers_key,
            encoder: (self.transformer, self.encoder),
        };

        input
            .filter_map(|event| {
                // Compute the topic.
                future::ready(
                    self.topic
                        .render_string(&event)
                        .map_err(|error| {
                            emit!(TemplateRenderingError {
                                field: None,
                                drop_event: true,
                                error,
                            });
                        })
                        .ok()
                        .map(|topic| (topic, event)),
                )
            })
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol("kafka")
            .run()
            .await
    }
}

pub(crate) async fn healthcheck(
    config: KafkaSinkConfig,
    healthcheck_options: SinkHealthcheckOptions,
    #[cfg(feature = "aws-core")] msk_iam: Option<crate::kafka::MskIamCredentials>,
) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client_config = config.to_rdkafka().unwrap();
    let topic: Option<String> = match config.healthcheck_topic {
        Some(topic) => Some(topic),
        _ => match config.topic.render_string(&LogEvent::from_str_legacy("")) {
            Ok(topic) => Some(topic),
            Err(error) => {
                warn!(
                    message = "Could not generate topic for healthcheck.",
                    %error,
                );
                None
            }
        },
    };

    // Build the client context in the async context (so the tokio runtime handle is captured for
    // the OAUTHBEARER token callback), then move it into the blocking task.
    #[cfg(feature = "aws-core")]
    let context = KafkaStatisticsContext::new(false, Span::current(), msk_iam);
    #[cfg(not(feature = "aws-core"))]
    let context = KafkaStatisticsContext::new(false, Span::current());

    tokio::task::spawn_blocking(move || {
        let producer: BaseProducer<_> = client_config.create_with_context(context).unwrap();
        let topic = topic.as_deref();

        producer
            .client()
            .fetch_metadata(topic, healthcheck_options.timeout)
            .map(|_| ())
    })
    .await??;
    trace!("Healthcheck completed.");
    Ok(())
}

#[async_trait]
impl StreamSink<Event> for KafkaSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
