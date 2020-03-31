#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    sinks::util::{encoding::EncodingConfig, tcp::TcpSinkConfig, udp::UdpSinkConfig, Encoding},
    tls::TlsConfig,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SocketSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(UdpSinkConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

inventory::submit! {
    SinkDescription::new_without_default::<SocketSinkConfig>("socket")
}

impl SocketSinkConfig {
    pub fn make_tcp_config(
        address: String,
        encoding: EncodingConfig<Encoding>,
        tls: Option<TlsConfig>,
    ) -> Self {
        TcpSinkConfig {
            address,
            encoding,
            tls,
        }
        .into()
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        TcpSinkConfig::new(address, EncodingConfig::from(Encoding::Text)).into()
    }
}

impl From<TcpSinkConfig> for SocketSinkConfig {
    fn from(config: TcpSinkConfig) -> Self {
        Self {
            mode: Mode::Tcp(config),
        }
    }
}

impl From<UdpSinkConfig> for SocketSinkConfig {
    fn from(config: UdpSinkConfig) -> Self {
        Self {
            mode: Mode::Udp(config),
        }
    }
}

#[typetag::serde(name = "socket")]
impl SinkConfig for SocketSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        match &self.mode {
            Mode::Tcp(config) => config.build(cx),
            Mode::Udp(config) => config.build(cx),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        event::Event,
        test_util::{next_addr, runtime},
        topology::config::SinkContext,
    };
    use futures01::Sink;
    use serde_json::Value;
    use std::net::UdpSocket;

    #[test]
    fn udp_message() {
        let addr = next_addr();
        let receiver = UdpSocket::bind(addr).unwrap();

        let config = SocketSinkConfig {
            mode: Mode::Udp(UdpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Json.into(),
            }),
        };
        let mut rt = runtime();
        let context = SinkContext::new_test(rt.executor());
        let (sink, _healthcheck) = config.build(context).unwrap();

        let event = Event::from("raw log line");
        let pump = sink.send(event.clone());
        rt.block_on(pump).unwrap();

        let mut buf = [0; 256];
        let (size, _src_addr) = receiver
            .recv_from(&mut buf)
            .expect("Did not receive message");

        let packet = String::from_utf8(buf[..size].to_vec()).expect("Invalid data received");
        let data = serde_json::from_str::<Value>(&packet).expect("Invalid JSON received");
        let data = data.as_object().expect("Not a JSON object");
        assert!(data.get("timestamp").is_some());
        let message = data.get("message").expect("No message in JSON");
        assert_eq!(message, &Value::String("raw log line".into()));
    }
}
