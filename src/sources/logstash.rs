use super::util::{SocketListenAddr, TcpIsErrorFatal, TcpSource};
use crate::{
    config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::io;
use tokio_util::codec::Decoder;

#[derive(Deserialize, Serialize, Debug)]
pub struct LogstashConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    receive_buffer_bytes: Option<usize>,
}

inventory::submit! {
    SourceDescription::new::<LogstashConfig>("syslog")
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:514".parse().unwrap()),
            keepalive: None,
            tls: None,
            receive_buffer_bytes: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logstash")]
impl SourceConfig for LogstashConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = LogstashSource {};
        let shutdown_secs = 30;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        source.run(
            self.address,
            self.keepalive,
            shutdown_secs,
            tls,
            self.receive_buffer_bytes,
            cx.shutdown,
            cx.out,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "logstash"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct LogstashSource {}

impl TcpSource for LogstashSource {
    type Error = DecodeError;
    type Decoder = LogstashDecoder;

    fn decoder(&self) -> Self::Decoder {
        LogstashDecoder::new()
    }

    fn build_event(&self, _frame: String, _host: Bytes) -> Option<Event> {
        None
    }
}

#[derive(Clone, Debug)]
struct LogstashDecoder {}

impl LogstashDecoder {
    fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub enum DecodeError {
    IO(io::Error),
}

impl TcpIsErrorFatal for DecodeError {
    fn is_error_fatal() -> bool {
        // TODO
        true
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::IO(err) => write!(f, "{}", err),
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(e: io::Error) -> Self {
        DecodeError::IO(e)
    }
}

impl Decoder for LogstashDecoder {
    type Item = String;
    type Error = DecodeError;

    fn decode(&mut self, _src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogstashConfig>();
    }
}
