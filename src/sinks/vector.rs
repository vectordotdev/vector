use crate::{
    buffers::Acker,
    event::proto,
    sinks::util::tcp::TcpSink,
    sinks::util::SinkExt,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures::{future, Future, Sink};
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::net::SocketAddr;
use tokio::net::TcpStream;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorSinkConfig {
    pub address: String,
}

impl VectorSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address }
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

        let ip_addr = cx
            .resolver()
            .lookup_ip(uri.host().ok_or(BuildError::MissingHost)?)
            // This is fine to do here because this is just receiving on a channel
            // and does not require access to the reactor/timer.
            .wait()
            .context(super::DNSError)?
            .next()
            .ok_or(Box::new(super::BuildError::DNSFailure {
                address: self.address.clone(),
            }))?;

        let port = uri.port_part().ok_or(BuildError::MissingPort)?.as_u16();
        let addr = SocketAddr::new(ip_addr, port);

        let sink = vector(self.address.clone(), addr, cx.acker());
        let healthcheck = super::util::tcp::tcp_healthcheck(addr);

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "vector"
    }
}

pub fn vector(hostname: String, addr: SocketAddr, acker: Acker) -> super::RouterSink {
    Box::new(
        TcpSink::new(hostname, addr, None)
            .stream_ack(acker)
            .with(encode_event),
    )
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

pub fn vector_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|err| err.into())
    });

    Box::new(check)
}

fn encode_event(event: Event) -> Result<Bytes, ()> {
    let event = proto::EventWrapper::from(event);
    let event_len = event.encoded_len() as u32;
    let full_len = event_len + 4;

    let mut out = BytesMut::with_capacity(full_len as usize);
    out.put_u32_be(event_len);
    event.encode(&mut out).unwrap();
    Ok(out.freeze())
}
