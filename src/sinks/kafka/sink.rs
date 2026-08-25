use std::time::{Duration, Instant};

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
    kafka::{KafkaHealthcheckContext, KafkaStatisticsContext, MskIamTokenProvider},
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
    topic: ConfinedTemplate,
    key_field: Option<OwnedTargetPath>,
    headers_key: Option<OwnedTargetPath>,
}

pub(crate) fn create_producer(
    client_config: ClientConfig,
    msk_iam_token_provider: Option<MskIamTokenProvider>,
) -> crate::Result<FutureProducer<KafkaStatisticsContext>> {
    let producer = client_config
        .create_with_context(KafkaStatisticsContext {
            expose_lag_metrics: false,
            span: Span::current(),
            msk_iam_token_provider,
        })
        .context(KafkaCreateFailedSnafu)?;
    Ok(producer)
}

impl KafkaSink {
    pub(crate) fn new(config: KafkaSinkConfig, topic: ConfinedTemplate) -> crate::Result<Self> {
        let producer_config = config.to_rdkafka()?;
        let producer = create_producer(producer_config, config.auth.msk_iam_token_provider())?;
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
            topic,
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
    topic_template: ConfinedTemplate,
    healthcheck_options: SinkHealthcheckOptions,
) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client_config = config.to_rdkafka().unwrap();
    let topic: Option<String> = match config.healthcheck_topic {
        Some(topic) => Some(topic),
        _ => match topic_template.render_string(&LogEvent::from_str_legacy("")) {
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

    let msk_iam_token_provider = config.auth.msk_iam_token_provider();
    tokio::task::spawn_blocking(move || -> crate::Result<()> {
        // One deadline bounds the whole healthcheck (token priming plus metadata fetch) so a
        // slow token cold start cannot stack a second full timeout on top of the first.
        let deadline = Instant::now() + healthcheck_options.timeout;
        let producer: BaseProducer<KafkaHealthcheckContext> =
            client_config.create_with_context(KafkaHealthcheckContext {
                msk_iam_token_provider: msk_iam_token_provider.clone(),
            })?;
        if let Some(token_provider) = msk_iam_token_provider {
            // Serve the initial OAuth token refresh event so an MSK IAM token is set before
            // connecting to fetch metadata. librdkafka emits the refresh event asynchronously
            // shortly after client creation and the token generation callback runs
            // synchronously within `poll`, so poll until a token has been generated or the
            // deadline elapses. Note a `poll` that dispatches the token callback blocks
            // until token generation completes or times out, so the deadline can be
            // overshot by that much.
            while !token_provider.token_generated() && Instant::now() < deadline {
                producer.poll(Duration::from_millis(100));
            }
            // Without a token, SASL authentication cannot even begin, so `fetch_metadata`
            // could only report a generic transport failure. Failing here attributes the
            // problem to token generation instead.
            if !token_provider.token_generated() {
                return Err(format!(
                    "MSK IAM token was not generated within the healthcheck timeout ({:?}); \
                     see any preceding token generation errors",
                    healthcheck_options.timeout
                )
                .into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining < healthcheck_options.timeout / 2 {
                warn!(
                    message = "MSK IAM token generation consumed most of the healthcheck \
                        timeout. If the subsequent metadata fetch times out, consider raising \
                        `healthcheck.timeout` to accommodate slow credential resolution.",
                    remaining_timeout = ?remaining,
                );
            }
        }
        let topic = topic.as_deref();

        producer
            .client()
            .fetch_metadata(topic, deadline.saturating_duration_since(Instant::now()))?;
        Ok(())
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
