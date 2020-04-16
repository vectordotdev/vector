use crate::{
    event::proto,
    internal_events::VectorEventSent,
    sinks::util::{tcp::TcpSink, StreamSink},
    tls::{MaybeTlsSettings, TlsConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures01::{stream::iter_ok, Sink};
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorSinkConfig {
    pub address: String,
    pub tls: Option<TlsConfig>,
}

impl VectorSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address, tls: None }
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
}

inventory::submit! {
    SinkDescription::new_without_default::<VectorSinkConfig>("vector")
}

#[typetag::serde(name = "vector")]
impl SinkConfig for VectorSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(BuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(BuildError::MissingPort)?;

        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let sink = TcpSink::new(host.clone(), port, cx.resolver(), tls);
        let sink = StreamSink::new(sink, cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event)));
        let healthcheck = super::util::tcp::tcp_healthcheck(host, port, cx.resolver());

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "vector"
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

fn encode_event(event: Event) -> Option<Bytes> {
    let event = proto::EventWrapper::from(event);
    let event_len = event.encoded_len();
    let full_len = event_len + 4;

    emit!(VectorEventSent {
        byte_size: full_len
    });

    let mut out = BytesMut::with_capacity(full_len);
    out.put_u32_be(event_len as u32);
    event.encode(&mut out).unwrap();
    Some(out.freeze())
}
