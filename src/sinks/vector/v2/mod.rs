use snafu::Snafu;

mod config;
mod service;
mod sink;

pub use config::VectorConfig;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum VectorSinkError {
    #[snafu(display("Request failed: {}", source))]
    Request { source: tonic::Status },

    #[snafu(display("Vector source unhealthy"))]
    Health,

    #[snafu(display("URL has no host."))]
    NoHost,
}

#[cfg(test)]
mod tests {
    use bytes::{BufMut, Bytes, BytesMut};
    use futures::{channel::mpsc, StreamExt};
    use http::request::Parts;
    use hyper::Method;
    use prost::Message;
    use vector_core::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::{
        config::SinkContext,
        event::Event,
        proto::vector as proto,
        sinks::{util::test::build_test_server_generic, vector::v2::config::with_default_scheme},
        test_util::{components, next_addr, random_lines_with_stream},
    };

    // one byte for the compression flag plus four bytes for the length
    const GRPC_HEADER_SIZE: usize = 5;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VectorConfig>();
    }

    #[tokio::test]
    async fn deliver_message() {
        let num_lines = 10;

        let in_addr = next_addr();

        let config = format!(r#"address = "http://{}/""#, in_addr);
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        components::init_test();
        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
        components::SINK_TESTS.assert(&components::HTTP_SINK_TAGS);

        let output_lines = get_received(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/vector.Vector/PushEvents", parts.uri.path());
            assert_eq!(
                "application/grpc",
                parts.headers.get("content-type").unwrap().to_str().unwrap()
            );
        })
        .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn acknowledges_error() {
        let num_lines = 10;

        let in_addr = next_addr();

        let config = format!(r#"address = "http://{}/""#, in_addr);
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        let (_rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "7") // permission denied
                .header("content-type", "application/grpc")
                .body(tonic::body::empty_body())
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[test]
    fn test_with_default_scheme() {
        assert_eq!(
            with_default_scheme("0.0.0.0", false).unwrap().to_string(),
            "http://0.0.0.0/"
        );
        assert_eq!(
            with_default_scheme("0.0.0.0", true).unwrap().to_string(),
            "https://0.0.0.0/"
        );
    }

    async fn get_received(
        rx: mpsc::Receiver<(Parts, Bytes)>,
        assert_parts: impl Fn(Parts),
    ) -> Vec<String> {
        rx.map(|(parts, body)| {
            assert_parts(parts);

            let proto_body = body.slice(GRPC_HEADER_SIZE..);

            let req = proto::PushEventsRequest::decode(proto_body).unwrap();

            let mut events = Vec::with_capacity(req.events.len());
            for event in req.events {
                let event: Event = event.into();
                let string = event.as_log().get("message").unwrap().to_string_lossy();
                events.push(string)
            }

            events
        })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
    }

    // taken from <https://github.com/hyperium/tonic/blob/5aa8ae1fec27377cd4c2a41d309945d7e38087d0/examples/src/grpc-web/client.rs#L45-L75>
    fn encode_body<T>(msg: T) -> Bytes
    where
        T: prost::Message,
    {
        let mut buf = BytesMut::with_capacity(1024);

        // first skip past the header
        // cannot write it yet since we don't know the size of the
        // encoded message
        buf.reserve(GRPC_HEADER_SIZE);
        unsafe {
            buf.advance_mut(GRPC_HEADER_SIZE);
        }

        // write the message
        msg.encode(&mut buf).unwrap();

        // now we know the size of encoded message and can write the
        // header
        let len = buf.len() - GRPC_HEADER_SIZE;
        {
            let mut buf = &mut buf[..GRPC_HEADER_SIZE];

            // compression flag, 0 means "no compression"
            buf.put_u8(0);

            buf.put_u32(len as u32);
        }

        buf.split_to(len + GRPC_HEADER_SIZE).freeze()
    }
}
