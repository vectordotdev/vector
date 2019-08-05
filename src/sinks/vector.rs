use crate::{
    buffers::Acker,
    event::proto,
    sinks::tcp::{TcpSink, TcpSinkTls},
    sinks::util::SinkExt,
    topology::config::{DataType, SinkConfig},
    Event,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures::{future, Future, Sink};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, ToSocketAddrs};
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

#[typetag::serde(name = "vector")]
impl SinkConfig for VectorSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let addr = self
            .address
            .to_socket_addrs()
            .map_err(|e| format!("IO Error: {}", e))?
            .next()
            .ok_or_else(|| "Unable to resolve DNS for provided address".to_string())?;

        let sink = vector(addr, acker);
        let healthcheck = super::tcp::tcp_healthcheck(addr);

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

pub fn vector(addr: SocketAddr, acker: Acker) -> super::RouterSink {
    Box::new(
        TcpSink::new(addr, TcpSinkTls::default())
            .stream_ack(acker)
            .with(move |event| encode_event(event)),
    )
}

pub fn vector_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|err| err.to_string())
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
