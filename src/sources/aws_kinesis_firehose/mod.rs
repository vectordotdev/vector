use std::{convert::Infallible, fmt, net::SocketAddr, time::Duration};

use futures::FutureExt;
use hyper::{Server, service::make_service_fn};
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
    sensitive_string::SensitiveString,
    tls::MaybeTlsIncomingStream,
};
use vrl::value::{Kind, kind::Collection};

use crate::{
    codecs::DecodingConfig,
    config::{
        GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
    },
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::http_server::{build_param_matcher, remove_duplicates},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

pub mod errors;
mod filters;
mod handlers;
mod models;

/// Configuration for the `aws_kinesis_firehose` source.
#[configurable_component(source(
    "aws_kinesis_firehose",
    "Collect logs from AWS Kinesis Firehose."
))]
#[derive(Clone, Debug)]
pub struct AwsKinesisFirehoseConfig {
    /// The socket address to listen for connections on.
    #[configurable(metadata(docs::examples = "0.0.0.0:443"))]
    #[configurable(metadata(docs::examples = "localhost:443"))]
    address: SocketAddr,

    /// An access key to authenticate requests against.
    ///
    /// AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
    /// configured, `access_key` should be set to the same value. Otherwise, all requests are allowed.
    #[configurable(deprecated = "This option has been deprecated, use `access_keys` instead.")]
    #[configurable(metadata(docs::examples = "A94A8FE5CCB19BA61C4C08"))]
    access_key: Option<SensitiveString>,

    /// A list of access keys to authenticate requests against.
    ///
    /// AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
    /// configured, `access_keys` should be set to the same value. Otherwise, all requests are allowed.
    #[configurable(metadata(docs::examples = "access_keys_example()"))]
    access_keys: Option<Vec<SensitiveString>>,

    /// Whether or not to store the AWS Firehose Access Key in event secrets.
    ///
    /// If set to `true`, when incoming requests contains an access key sent by AWS Firehose, it is kept in the
    /// event secrets as "aws_kinesis_firehose_access_key".
    #[configurable(derived)]
    store_access_key: bool,

    /// The compression scheme to use for decompressing records within the Firehose message.
    ///
    /// Some services, like AWS CloudWatch Logs, [compresses the events with gzip][events_with_gzip],
    /// before sending them AWS Kinesis Firehose. This option can be used to automatically decompress
    /// them before forwarding them to the next component.
    ///
    /// Note that this is different from [Content encoding option][encoding_option] of the
    /// Firehose HTTP endpoint destination. That option controls the content encoding of the entire HTTP request.
    ///
    /// [events_with_gzip]: https://docs.aws.amazon.com/firehose/latest/dev/writing-with-cloudwatch-logs.html
    /// [encoding_option]: https://docs.aws.amazon.com/firehose/latest/dev/create-destination.html#create-destination-http
    #[serde(default)]
    record_compression: Compression,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,

    /// A list of attributes from X-Amz-Firehose-Common-Attributes header to include in the log event.
    ///
    /// Accepts the wildcard (`*`) character for attributes matching a specified pattern.
    ///
    /// Specifying "*" results in all common attributes included in the log event.
    ///
    /// Legacy namespace: selected attributes are added under the root `common_attributes` object
    /// Vector namespace: selected attributes are added under the source metadata at `aws_kinesis_firehose.common_attributes`
    #[serde(default)]
    #[configurable(metadata(docs::examples = "environment"))]
    #[configurable(metadata(docs::examples = "application_group"))]
    #[configurable(metadata(docs::examples = "application_*"))]
    #[configurable(metadata(docs::examples = "*"))]
    common_attributes: Vec<String>,
}

const fn access_keys_example() -> [&'static str; 2] {
    ["A94A8FE5CCB19BA61C4C08", "B94B8FE5CCB19BA61C4C12"]
}

/// Compression scheme for records in a Firehose message.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the object is determined by looking at its file signature, also known
    /// as [magic bytes][magic_bytes].
    ///
    /// If the record fails to decompress with the discovered format, the record is forwarded as is.
    /// Thus, if you know the records are always gzip encoded (for example, if they are coming from AWS CloudWatch Logs),
    /// set `gzip` in this field so that any records that are not-gzipped are rejected.
    ///
    /// [magic_bytes]: https://en.wikipedia.org/wiki/List_of_file_signatures
    #[default]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,
}

impl fmt::Display for Compression {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Compression::Auto => write!(fmt, "auto"),
            Compression::None => write!(fmt, "none"),
            Compression::Gzip => write!(fmt, "gzip"),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SourceConfig for AwsKinesisFirehoseConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        if self.access_key.is_some() {
            warn!("DEPRECATION `access_key`, use `access_keys` instead.")
        }

        // Merge with legacy `access_key`
        let access_keys = self
            .access_keys
            .iter()
            .flatten()
            .chain(self.access_key.iter());

        let common_attributes = build_param_matcher(&remove_duplicates(
            self.common_attributes.clone(),
            "common_attributes",
        ))?;

        let svc = filters::firehose(
            access_keys.map(|key| key.inner().to_string()).collect(),
            self.store_access_key,
            self.record_compression,
            decoder,
            acknowledgements,
            cx.out,
            log_namespace,
            common_attributes,
        );

        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), true)?;
        let listener = tls.bind(&self.address).await?;

        let keepalive_settings = self.keepalive.clone();
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let svc = ServiceBuilder::new()
                    .layer(build_http_trace_layer(span.clone()))
                    .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                        MaxConnectionAgeLayer::new(
                            Duration::from_secs(secs),
                            keepalive_settings.max_connection_age_jitter_factor,
                            conn.peer_addr(),
                        )
                    }))
                    .service(warp::service(svc.clone()));
                futures_util::future::ok::<_, Infallible>(svc)
            });

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(make_svc)
                .with_graceful_shutdown(shutdown.map(|_| ()))
                .await
                .map_err(|err| {
                    error!("An error occurred: {:?}.", err);
                })?;

            Ok(())
        }))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let common_attributes_path = (!self.common_attributes.is_empty()).then_some(
            LegacyKey::InsertIfEmpty(owned_value_path!("common_attributes")),
        );
        let schema_definition = self
            .decoding
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("request_id"))),
                &owned_value_path!("request_id"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("source_arn"))),
                &owned_value_path!("source_arn"),
                Kind::bytes(),
                None,
            )
            // for common attributes dynamically added from X-Amz-Firehose-Common-Attributes header
            .with_source_metadata(
                Self::NAME,
                common_attributes_path,
                &owned_value_path!("common_attributes"),
                Kind::object(Collection::from_unknown(
                    Kind::bytes().or_null().or_undefined(),
                ))
                .or_undefined(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl GenerateConfig for AwsKinesisFirehoseConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            address: "0.0.0.0:443".parse().unwrap(),
            access_key: None,
            access_keys: None,
            store_access_key: false,
            tls: None,
            record_compression: Default::default(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: Default::default(),
            log_namespace: None,
            keepalive: Default::default(),
            common_attributes: vec![],
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests;
