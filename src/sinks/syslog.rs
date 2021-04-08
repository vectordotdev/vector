#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    Event,
    sinks::util::{
        tcp::TcpSinkConfig, udp::UdpSinkConfig,
    },
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SyslogSinkConfig {
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
    SinkDescription::new::<SyslogSinkConfig>("syslog")
}

impl GenerateConfig for SyslogSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"address = "2001:db8::1:514"
            mode = "tcp"
            "#,
        )
        .unwrap()
    }
}

impl SyslogSinkConfig {
    pub fn new(mode: Mode) -> Self {
        SyslogSinkConfig { mode }
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        Self::new(
            Mode::Tcp(TcpSinkConfig::from_address(address)),
        )
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SinkConfig for SyslogSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let syslog_encode = move |event| build_syslog_message(event);
        match &self.mode {
            Mode::Tcp(config) => config.build(cx, syslog_encode),
            Mode::Udp(config) => config.build(cx, syslog_encode),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx, syslog_encode),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "syslog"
    }
}

fn build_syslog_message(event: Event) -> Option<Bytes> {
    let log = event.into_log();
    // TODO: syslog conversion
    None
}
