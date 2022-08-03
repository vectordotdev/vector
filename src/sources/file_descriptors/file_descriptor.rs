use std::fs::File;
use std::io;
use std::os::unix::io::FromRawFd;

use super::FileDescriptorConfig;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use indoc::indoc;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

const NAME: &str = "file_descriptor";

use crate::{
    config::{GenerateConfig, Output, Resource, SourceConfig, SourceContext, SourceDescription},
    serde::default_decoding,
};
/// Configuration for the `file_descriptor` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileDescriptorSourceConfig {
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

    /// The file descriptor number to read from.
    pub fd: u32,
}

impl FileDescriptorConfig for FileDescriptorSourceConfig {
    fn host_key(&self) -> Option<String> {
        self.host_key.clone()
    }
    fn framing(&self) -> Option<FramingConfig> {
        self.framing.clone()
    }
    fn decoding(&self) -> DeserializerConfig {
        self.decoding.clone()
    }
    fn name(&self) -> String {
        NAME.to_string()
    }
    fn description(&self) -> String {
        format!("file descriptor {}", self.fd)
    }
}

inventory::submit! {
    SourceDescription::new::<FileDescriptorSourceConfig>(NAME)
}

impl GenerateConfig for FileDescriptorSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            fd = 10
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "file_descriptor")]
impl SourceConfig for FileDescriptorSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let pipe = io::BufReader::new(unsafe { File::from_raw_fd(self.fd as i32) });
        self.source(pipe, cx.shutdown, cx.out)
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        NAME
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Fd(self.fd)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use nix::unistd::{close, pipe, write};

    use super::*;
    use crate::{
        config::log_schema, test_util::components::assert_source_compliance, SourceSender,
    };
    use futures::StreamExt;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileDescriptorSourceConfig>();
    }

    #[tokio::test]
    async fn file_descriptor_decodes_line() {
        assert_source_compliance(&["protocol"], async {
            let (tx, rx) = SourceSender::new_test();
            let (read_fd, write_fd) = pipe().unwrap();
            let config = FileDescriptorSourceConfig {
                max_length: crate::serde::default_max_length(),
                host_key: Default::default(),
                framing: None,
                decoding: default_decoding(),
                fd: read_fd as u32,
            };

            let mut stream = rx;

            write(write_fd, b"hello world\nhello world again\n").unwrap();
            close(write_fd).unwrap();

            let context = SourceContext::new_test(tx, None);
            config.build(context).await.unwrap().await.unwrap();

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

    #[tokio::test]
    async fn file_descriptor_handles_invalid_fd() {
        let (tx, rx) = SourceSender::new_test();
        let (_read_fd, write_fd) = pipe().unwrap();
        let config = FileDescriptorSourceConfig {
            max_length: crate::serde::default_max_length(),
            host_key: Default::default(),
            framing: None,
            decoding: default_decoding(),
            fd: write_fd as u32, // intentionally giving the source a write-only fd
        };

        let mut stream = rx;

        write(write_fd, b"hello world\nhello world again\n").unwrap();
        close(write_fd).unwrap();

        let context = SourceContext::new_test(tx, None);
        config.build(context).await.unwrap().await.unwrap();

        // The error "Bad file descriptor" will be logged when the source attempts to read
        // for the first time.
        let event = stream.next().await;
        assert!(event.is_none());
    }
}
