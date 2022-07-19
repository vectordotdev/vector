use std::fs::File;
use std::io;
use std::os::raw::c_int;
use std::os::unix::io::FromRawFd;

use super::util::file_descriptor::{file_descriptor_source, FileDescriptorConfig};
use codecs::decoding::{DeserializerConfig, FramingConfig};
use indoc::indoc;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{GenerateConfig, Output, Resource, SourceConfig, SourceContext, SourceDescription},
    serde::default_decoding,
    shutdown::ShutdownSignal,
    SourceSender,
};
/// Configuration for the `pipe` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PipeConfig {
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
    pub fd: c_int,
}

impl FileDescriptorConfig for PipeConfig {
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
        format!("fd {}", self.fd)
    }
}

inventory::submit! {
    SourceDescription::new::<PipeConfig>("pipe")
}

impl GenerateConfig for PipeConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            fd = 10
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipe")]
impl SourceConfig for PipeConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        pipe_source(self.clone(), cx.shutdown, cx.out)
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        "pipe"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Fd(self.fd)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub fn pipe_source(
    config: PipeConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<super::Source> {
    let pipe = io::BufReader::new(unsafe { File::from_raw_fd(config.fd) });
    file_descriptor_source(pipe, config, shutdown, out, "pipe")
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
        crate::test_util::test_generate_config::<PipeConfig>();
    }

    #[tokio::test]
    async fn pipe_decodes_line() {
        assert_source_compliance(&["protocol"], async {
            let (tx, rx) = SourceSender::new_test();
            let (read_fd, write_fd) = pipe().unwrap();
            let config = PipeConfig {
                max_length: crate::serde::default_max_length(),
                host_key: Default::default(),
                framing: None,
                decoding: default_decoding(),
                fd: read_fd,
            };

            let mut stream = rx;

            write(write_fd, b"hello world\nhello world again\n").unwrap();
            close(write_fd).unwrap();

            pipe_source(config, ShutdownSignal::noop(), tx)
                .unwrap()
                .await
                .unwrap();

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
    async fn pipe_handles_invalid_fd() {
        let (tx, rx) = SourceSender::new_test();
        let (_read_fd, write_fd) = pipe().unwrap();
        let config = PipeConfig {
            max_length: crate::serde::default_max_length(),
            host_key: Default::default(),
            framing: None,
            decoding: default_decoding(),
            fd: write_fd, // intentionalally giving the source a write-only fd
        };

        let mut stream = rx;

        write(write_fd, b"hello world\nhello world again\n").unwrap();
        close(write_fd).unwrap();

        pipe_source(config, ShutdownSignal::noop(), tx)
            .unwrap()
            .await
            .unwrap();

        // The error "Bad file descriptor" will be logged when the source attempts to read
        // for the first time.
        let event = stream.next().await;
        assert!(event.is_none());
    }
}
