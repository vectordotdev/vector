//! TCP QoS helpers to apply DSCP TOS on newly created sockets.
use std::net::TcpStream;
use crate::net::qos::set_stream_tos;

pub fn apply_qos(stream: &TcpStream, tos: Option<u8>) {
    if let Some(t) = tos {
        if let Err(e) = set_stream_tos(stream, t) {
            let _ = e;
        }
    }
}
