use bytes::{BufMut, BytesMut};
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio_util::codec::Encoder;
use vector_core::event::{proto, Event};

use crate::{
    config::{GenerateConfig, SinkContext},
    sinks::{util::tcp::TcpSinkConfig, Healthcheck, VectorSink},
    tcp::TcpKeepaliveConfig,
    tls::TlsEnableableConfig,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: String,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsEnableableConfig>,
    send_buffer_bytes: Option<usize>,
}

impl VectorConfig {
    pub fn set_tls(&mut self, config: Option<TlsEnableableConfig>) {
        self.tls = config;
    }

    pub const fn new(
        address: String,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsEnableableConfig>,
        send_buffer_bytes: Option<usize>,
    ) -> Self {
        Self {
            address,
            keepalive,
            tls,
            send_buffer_bytes,
        }
    }

    pub const fn from_address(address: String) -> Self {
        Self::new(address, None, None, None)
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::new("127.0.0.1:5000".to_string(), None, None, None)).unwrap()
    }
}

impl VectorConfig {
    pub(crate) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink_config = TcpSinkConfig::new(
            self.address.clone(),
            self.keepalive,
            self.tls.clone(),
            self.send_buffer_bytes,
        );
        sink_config.build(cx, Default::default(), VectorEncoder)
    }
}

#[derive(Debug, Clone)]
struct VectorEncoder;

impl Encoder<Event> for VectorEncoder {
    type Error = codecs::encoding::Error;

    fn encode(&mut self, event: Event, out: &mut BytesMut) -> Result<(), Self::Error> {
        let data = proto::EventWrapper::from(event);
        let event_len = data.encoded_len();
        let full_len = event_len + 4;

        let capacity = out.capacity();
        if capacity < full_len {
            out.reserve(full_len - capacity);
        }
        out.put_u32(event_len as u32);
        data.encode(out).unwrap();

        Ok(())
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
