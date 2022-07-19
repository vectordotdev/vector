use std::io;

use codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{Output, Resource, SourceConfig, SourceContext, SourceDescription},
    serde::default_decoding,
    shutdown::ShutdownSignal,
    SourceSender,
};

use super::util::file_descriptor::{file_descriptor_source, FileDescriptorConfig};

/// Configuration for the `stdin` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    /// The maximum buffer size, in bytes, of incoming messages.
    ///
    /// Messages larger than this are truncated.
    #[serde(default = "crate::serde::default_max_length")]
    pub max_length: usize,

    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    /// The value will be the current hostname for wherever Vector is running.
    ///
    /// By default, the [global `host_key` option](https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key) is used.
    pub host_key: Option<String>,

    #[configurable(derived)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,
}

impl FileDescriptorConfig for StdinConfig {
    fn host_key(&self) -> Option<String> {
        self.host_key.clone()
    }
    fn framing(&self) -> Option<FramingConfig> {
        self.framing.clone()
    }
    fn decoding(&self) -> DeserializerConfig {
        self.decoding.clone()
    }
    fn description(&self) -> String {
        "stdin".to_string()
    }
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: crate::serde::default_max_length(),
            host_key: Default::default(),
            framing: None,
            decoding: default_decoding(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<StdinConfig>("stdin")
}

impl_generate_config_from_default!(StdinConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        stdin_source(
            io::BufReader::new(io::stdin()),
            self.clone(),
            cx.shutdown,
            cx.out,
        )
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Fd(0)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub fn stdin_source<R>(
    stdin: R,
    config: StdinConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<super::Source>
where
    R: Send + io::BufRead + 'static,
{
    file_descriptor_source(stdin, config, shutdown, out, "stdin")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{
        config::log_schema, test_util::components::assert_source_compliance, SourceSender,
    };
    use futures::StreamExt;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StdinConfig>();
    }

    #[tokio::test]
    async fn stdin_decodes_line() {
        assert_source_compliance(&["protocol"], async {
            let (tx, rx) = SourceSender::new_test();
            let config = StdinConfig::default();
            let buf = Cursor::new("hello world\nhello world again");

            stdin_source(buf, config, ShutdownSignal::noop(), tx)
                .unwrap()
                .await
                .unwrap();

            let mut stream = rx;

            let event = stream.next().await;
            assert_eq!(
                Some("hello world".into()),
                event.map(|event| event.as_log()[log_schema().message_key()].to_string_lossy())
            );

            let event = stream.next().await;
            assert_eq!(
                Some("hello world again".into()),
                event.map(|event| event.as_log()[log_schema().message_key()].to_string_lossy())
            );

            let event = stream.next().await;
            assert!(event.is_none());
        })
        .await;
    }
}
