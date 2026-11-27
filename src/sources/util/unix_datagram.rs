use std::{fs::remove_file, path::PathBuf};
use vector_lib::internal_event::{emit, register};

use bytes::BytesMut;
use tokio::net::UnixDatagram;
use tokio_util::codec::Decoder as _;
use tracing::{Instrument as _, field};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::StreamDecodingError,
    internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol},
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    internal_events::{
        SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
        UnixSocketDatagramTooLargeError, UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::{
        Source,
        util::{change_socket_permissions, unix::UNNAMED_SOCKET_HOST},
    },
};

/// Returns a `Source` object corresponding to a Unix domain datagram socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
///
/// Datagrams longer than `max_length + 1` bytes, which leaves room for a trailing delimiter, are
/// truncated. The complete frames they contain are still decoded, but their incomplete final frame
/// is discarded and reported.
///
/// # Errors
///
/// The returned source reports an error if processing the datagram stream fails.
///
/// # Panics
///
/// The returned source panics if binding the socket or setting its permissions fails.
pub fn build_unix_datagram_source(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    decoder: Decoder,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    Ok(Box::pin(async move {
        let socket = UnixDatagram::bind(&listen_path).expect("Failed to bind to datagram socket");
        info!(message = "Listening.", path = ?listen_path, r#type = "unix_datagram");

        change_socket_permissions(&listen_path, socket_file_mode)
            .expect("Failed to set socket permissions");

        let result = listen(socket, max_length, decoder, shutdown, handle_events, out).await;

        // Delete socket file.
        if let Err(error) = remove_file(&listen_path) {
            emit(UnixSocketFileDeleteError {
                path: &listen_path,
                error,
            });
        }

        result
    }))
}

async fn listen(
    socket: UnixDatagram,
    max_length: usize,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    mut out: SourceSender,
) -> Result<(), crate::sources::SourceError> {
    // `max_length` excludes a trailing delimiter, so leave room for one. One more byte detects
    // datagrams that exceed the limit: the kernel only fills the whole buffer for those.
    let limit = max_length.saturating_add(1);
    let buffer_length = limit.saturating_add(1);
    let mut buf = BytesMut::with_capacity(buffer_length);
    let bytes_received = register(BytesReceived::from(Protocol::UNIX));
    loop {
        buf.resize(buffer_length, 0);
        tokio::select! {
            recv = socket.recv_from(&mut buf) => {
                let (byte_size, address) = recv.map_err(|error| {
                    let error = vector_lib::codecs::decoding::Error::FramingError(error.into());
                    emit(SocketReceiveError {
                        mode: SocketMode::Unix,
                        error: &error
                    });
                    crate::sources::SourceError::from(error)
                })?;

                let span = info_span!("datagram", peer_path = field::Empty);
                let received_from = if address.is_unnamed() {
                    // In most cases, we'll be connecting to this
                    // socket from an unnamed socket (a socket not
                    // bound to a file). Instead of a filename, we'll
                    // surface a specific host value.
                    span.record("peer_path", field::debug(UNNAMED_SOCKET_HOST));
                    Some(UNNAMED_SOCKET_HOST.to_owned())
                } else {
                    let path = address.as_pathname().map(ToOwned::to_owned).inspect(|path| {
                        span.record("peer_path", field::debug(path));
                    });

                    path.map(|p| p.to_string_lossy().into_owned())
                };

                let truncated = byte_size > limit;
                let byte_size = byte_size.min(limit);
                bytes_received.emit(ByteSize(byte_size));

                // Keep decoded events from retaining the socket's receive allocation.
                let payload = BytesMut::from(&buf[..byte_size]);

                async {
                    handle_datagram(payload, truncated, &decoder, &handle_events, received_from, &mut out)
                        .await;

                    if truncated {
                        emit(UnixSocketDatagramTooLargeError { limit });
                    }
                }
                .instrument(span)
                .await;
            }
            _ = &mut shutdown => return Ok(()),
        }
    }
}

/// The minimum number of bytes of a datagram fed to the framer at a time, as the `FramedRead` used
/// before did: some framers scan their whole buffer on every call, which would otherwise be
/// quadratic in the datagram size.
const DECODE_CHUNK_SIZE: usize = 8 * 1024;

/// Decodes the frames of a datagram and sends their events.
///
/// The final frame of a truncated datagram is incomplete, so it is discarded once the datagram
/// has yielded a complete frame. A truncated datagram without any frame boundary, such as one
/// decoded with `bytes` framing, is decoded as a whole, so the framer handles its oversized frame
/// as configured.
async fn handle_datagram(
    mut datagram: BytesMut,
    truncated: bool,
    decoder: &Decoder,
    handle_events: &impl Fn(&mut [Event], Option<String>),
    received_from: Option<String>,
    out: &mut SourceSender,
) {
    let mut decoder = decoder.clone();
    let mut payload = datagram.split_to(datagram.len().min(DECODE_CHUNK_SIZE));
    let mut at_eof = false;
    let mut found_frame = false;

    loop {
        let result = if at_eof {
            decoder.decode_eof(&mut payload)
        } else {
            decoder.decode(&mut payload)
        };

        match result {
            Ok(Some((mut events, _byte_size))) => {
                found_frame = true;

                emit(SocketEventsReceived {
                    mode: SocketMode::Unix,
                    byte_size: events.estimated_json_encoded_size_of(),
                    count: events.len(),
                });

                handle_events(&mut events, received_from.clone());

                let count = events.len();
                if (out.send_batch(events).await).is_err() {
                    emit(StreamClosedError { count });
                }
            }
            // Grow the chunk with the buffered frame, so that rescanning a long frame stays linear.
            // `split_to` leaves the chunks contiguous, so `unsplit` doesn't copy.
            Ok(None) if !datagram.is_empty() => {
                let chunk_size = payload.len().max(DECODE_CHUNK_SIZE);
                payload.unsplit(datagram.split_to(datagram.len().min(chunk_size)));
            }
            Ok(None) if at_eof || (truncated && found_frame) => break,
            Ok(None) => at_eof = true,
            Err(error) => {
                found_frame = true;
                emit(SocketReceiveError {
                    mode: SocketMode::Unix,
                    error: &error,
                });
                if !error.can_continue() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt as _;
    use vector_lib::codecs::{
        BytesDecoder, BytesDeserializer, CharacterDelimitedDecoder, NewlineDelimitedDecoder,
        OctetCountingDecoder, OversizedAction,
        decoding::{Deserializer, Framer},
    };

    use super::*;
    use crate::config::log_schema;

    /// Decodes `payload` with `framer`, returning the messages sent.
    async fn decode(payload: &[u8], truncated: bool, framer: Framer) -> Vec<String> {
        let (mut out, rx) = SourceSender::new_test();
        let decoder = Decoder::new(framer, Deserializer::Bytes(BytesDeserializer));
        handle_datagram(
            BytesMut::from(payload),
            truncated,
            &decoder,
            &|_: &mut [Event], _| {},
            None,
            &mut out,
        )
        .await;
        drop(out);

        rx.map(|event| {
            event.as_log()[log_schema().message_key().unwrap().to_string()]
                .to_string_lossy()
                .into_owned()
        })
        .collect()
        .await
    }

    fn newline_delimited() -> Framer {
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new())
    }

    #[tokio::test]
    async fn decodes_final_frame_of_complete_datagram() {
        assert_eq!(
            decode(b"one\ntwo", false, newline_delimited()).await,
            vec!["one", "two"]
        );
    }

    #[tokio::test]
    async fn discards_incomplete_final_frame_of_truncated_datagram() {
        assert_eq!(
            decode(b"one\ntw", true, newline_delimited()).await,
            vec!["one"]
        );
    }

    #[tokio::test]
    async fn keeps_complete_final_frame_of_truncated_datagram() {
        assert_eq!(
            decode(b"one\ntwo\n", true, newline_delimited()).await,
            vec!["one", "two"]
        );
    }

    #[tokio::test]
    async fn keeps_complete_octet_counted_frame_of_truncated_datagram() {
        let framer = Framer::OctetCounting(OctetCountingDecoder::new_with_max_length(20));
        assert_eq!(
            decode(b"5 hello12 abcdefghijk", true, framer).await,
            vec!["hello"]
        );
    }

    #[tokio::test]
    async fn decodes_frames_across_chunks() {
        // 80 frames of 204 bytes span two chunks and straddle their boundary, while staying below
        // the test sender's capacity, as nothing receives until all events are sent.
        let message = "a".repeat(200);
        let payload = format!("200 {message}").repeat(80);
        let framer = Framer::OctetCounting(OctetCountingDecoder::new_with_max_length(256));
        assert_eq!(
            decode(payload.as_bytes(), false, framer).await,
            vec![message; 80]
        );
    }

    #[tokio::test]
    async fn emits_datagram_larger_than_chunk_as_one_event() {
        let payload = "a".repeat(2 * DECODE_CHUNK_SIZE + 1);
        let framer = Framer::Bytes(BytesDecoder::new());
        assert_eq!(
            decode(payload.as_bytes(), false, framer).await,
            vec![payload]
        );
    }

    #[tokio::test]
    async fn emits_truncated_datagram_without_frame_boundaries() {
        let framer = Framer::Bytes(BytesDecoder::new());
        assert_eq!(decode(b"abc", true, framer).await, vec!["abc"]);
    }

    #[tokio::test]
    async fn truncates_oversized_single_frame_of_truncated_datagram() {
        let framer = Framer::CharacterDelimited(
            CharacterDelimitedDecoder::new_with_max_length(b'\n', 4)
                .with_oversized_action(OversizedAction::Truncate),
        );
        assert_eq!(decode(b"abcde", true, framer).await, vec!["abcd"]);
    }
}
