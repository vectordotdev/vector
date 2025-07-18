use std::io;

use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::OptionalValuePath;

use crate::{
    config::{Resource, SourceConfig, SourceContext, SourceOutput},
    serde::default_decoding,
};

use super::{outputs, FileDescriptorConfig};

/// Configuration for the `stdin` source.
#[configurable_component(source("stdin", "Collect logs sent via stdin."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    /// The maximum buffer size, in bytes, of incoming messages.
    ///
    /// Messages larger than this are truncated.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[serde(default = "crate::serde::default_max_length")]
    pub max_length: usize,

    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    pub host_key: Option<OptionalValuePath>,

    #[configurable(derived)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl FileDescriptorConfig for StdinConfig {
    fn host_key(&self) -> Option<OptionalValuePath> {
        self.host_key.clone()
    }

    fn framing(&self) -> Option<FramingConfig> {
        self.framing.clone()
    }

    fn decoding(&self) -> DeserializerConfig {
        self.decoding.clone()
    }

    fn description(&self) -> String {
        Self::NAME.to_string()
    }
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: crate::serde::default_max_length(),
            host_key: Default::default(),
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(StdinConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        self.source(
            io::BufReader::new(io::stdin()),
            cx.shutdown,
            cx.out,
            log_namespace,
        )
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        outputs(log_namespace, &self.host_key, &self.decoding, Self::NAME)
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Fd(0)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{
        config::log_schema, shutdown::ShutdownSignal,
        test_util::components::assert_source_compliance, test_util::components::SOURCE_TAGS,
        SourceSender,
    };
    use futures::StreamExt;
    use vector_lib::lookup::path;
    use vrl::value;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StdinConfig>();
    }

    #[tokio::test]
    async fn stdin_decodes_line() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let config = StdinConfig::default();
            let buf = Cursor::new("hello world\nhello world again");

            config
                .source(buf, ShutdownSignal::noop(), tx, LogNamespace::Legacy)
                .unwrap()
                .await
                .unwrap();

            let mut stream = rx;

            let event = stream.next().await;
            assert_eq!(
                Some("hello world".into()),
                event.map(
                    |event| event.as_log()[log_schema().message_key().unwrap().to_string()]
                        .to_string_lossy()
                        .into_owned()
                )
            );

            let event = stream.next().await;
            assert_eq!(
                Some("hello world again".into()),
                event.map(
                    |event| event.as_log()[log_schema().message_key().unwrap().to_string()]
                        .to_string_lossy()
                        .into_owned()
                )
            );

            let event = stream.next().await;
            assert!(event.is_none());
        })
        .await;
    }

    #[tokio::test]
    async fn stdin_decodes_line_vector_namespace() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tx, rx) = SourceSender::new_test();
            let config = StdinConfig::default();
            let buf = Cursor::new("hello world\nhello world again");

            config
                .source(buf, ShutdownSignal::noop(), tx, LogNamespace::Vector)
                .unwrap()
                .await
                .unwrap();

            let mut stream = rx;

            let event = stream.next().await;
            let event = event.unwrap();
            let log = event.as_log();
            let meta = log.metadata().value();

            assert_eq!(&value!("hello world"), log.value());
            assert_eq!(
                meta.get(path!("vector", "source_type")).unwrap(),
                &value!("stdin")
            );
            assert!(meta
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp());

            let event = stream.next().await;
            let event = event.unwrap();
            let log = event.as_log();

            assert_eq!(&value!("hello world again"), log.value());

            let event = stream.next().await;
            assert!(event.is_none());
        })
        .await;
    }
}
