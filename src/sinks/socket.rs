use crate::{
    sinks::util::tcp::{Encoding, TcpSinkConfig, TlsConfig},
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
}

inventory::submit! {
    SinkDescription::new_without_default::<SocketSinkConfig>("socket")
}

impl SocketSinkConfig {
    pub fn make_tcp_config(address: String, encoding: Encoding, tls: Option<TlsConfig>) -> Self {
        TcpSinkConfig {
            address,
            encoding,
            tls,
        }
        .into()
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        TcpSinkConfig::new(address).into()
    }
}

impl From<TcpSinkConfig> for SocketSinkConfig {
    fn from(config: TcpSinkConfig) -> Self {
        Self {
            mode: Mode::Tcp(config),
        }
    }
}

#[typetag::serde(name = "socket")]
impl SinkConfig for SocketSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        match &self.mode {
            Mode::Tcp(config) => config.build(cx),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "socket"
    }
}
