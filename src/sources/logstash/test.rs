use std::io::Write;

use bytes::BufMut;
use flate2::{Compression, write::ZlibEncoder};
use futures::{Stream, StreamExt, stream};
use rand::{RngExt, rng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vector_lib::codecs::ReadyFrames;
use vector_lib::lookup::OwnedTargetPath;
use vrl::event_path;
use vrl::value::kind::Collection;

use super::*;
use crate::{
    SourceSender,
    event::EventStatus,
    test_util::{
        addr::next_addr,
        components::{SOCKET_PUSH_SOURCE_TAGS, assert_source_compliance},
        spawn_collect_n, wait_for_tcp,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<LogstashConfig>();
}

#[tokio::test]
async fn test_delivered() {
    test_protocol(EventStatus::Delivered, true).await;
}

#[tokio::test]
async fn test_failed() {
    test_protocol(EventStatus::Rejected, false).await;
}

async fn start_logstash(status: EventStatus) -> (SocketAddr, impl Stream<Item = Event> + Unpin) {
    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (_guard, address) = next_addr();
    let source = LogstashConfig {
        address: address.into(),
        tls: None,
        permit_origin: None,
        keepalive: None,
        receive_buffer_bytes: None,
        acknowledgements: true.into(),
        connection_limit: None,
        tls_handshake_timeout_secs: None,
        log_namespace: None,
    }
    .build(SourceContext::new_test(sender, None))
    .await
    .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;
    (address, recv)
}

async fn test_protocol(status: EventStatus, sends_ack: bool) {
    let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
        let (address, recv) = start_logstash(status).await;
        spawn_collect_n(
            send_req(address, &[("message", "Hello, world!")], sends_ack),
            recv,
            1,
        )
        .await
    })
    .await;

    assert_eq!(events.len(), 1);
    let log = events[0].as_log();
    assert_eq!(
        log.get(event_path!("message")).unwrap().to_string_lossy(),
        "Hello, world!".to_string()
    );
    assert_eq!(
        log.get(event_path!("source_type"))
            .unwrap()
            .to_string_lossy(),
        "logstash".to_string()
    );
    assert!(log.get(event_path!("host")).is_some());
    assert!(log.get(event_path!("timestamp")).is_some());
}

fn push_req(req: &mut BytesMut, seq: u32, pairs: &[(&str, &str)]) {
    req.put_u8(b'2');
    req.put_u8(b'D');
    req.put_u32(seq);
    req.put_u32(pairs.len() as u32);
    for (key, value) in pairs {
        req.put_u32(key.len() as u32);
        req.put(key.as_bytes());
        req.put_u32(value.len() as u32);
        req.put(value.as_bytes());
    }
}

fn encode_req(seq: u32, pairs: &[(&str, &str)]) -> Bytes {
    let mut req = BytesMut::new();
    push_req(&mut req, seq, pairs);
    req.into()
}

fn push_window_size(req: &mut BytesMut, size: u32) {
    req.put_u8(b'2');
    req.put_u8(b'W');
    req.put_u32(size);
}

fn push_compressed(req: &mut BytesMut, inner: &[u8]) {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(inner).unwrap();
    let compressed = encoder.finish().unwrap();

    req.put_u8(b'2');
    req.put_u8(b'C');
    req.put_u32(compressed.len() as u32);
    req.put(compressed.as_slice());
}

fn decode_frames(mut src: BytesMut) -> Vec<(LogstashEventFrame, usize)> {
    let mut decoder = LogstashDecoder::new();
    let mut frames = Vec::new();

    while let Some(frame) = decoder.decode(&mut src).unwrap() {
        frames.push(frame);
    }

    assert_eq!(src.len(), 0);
    frames
}

fn decode_acknowledgements(mut ack: Bytes) -> Vec<u32> {
    let mut acknowledgements = Vec::new();

    while !ack.is_empty() {
        assert!(
            ack.len() >= 6,
            "ack stream ended with {} trailing bytes",
            ack.len()
        );
        assert_eq!(ack.get_u8(), b'2');
        assert_eq!(ack.get_u8(), b'A');
        acknowledgements.push(ack.get_u32());
    }

    acknowledgements
}

fn decoded_sequence_numbers(decoded: &[(LogstashEventFrame, usize)]) -> Vec<u32> {
    decoded
        .iter()
        .map(|(frame, _)| frame.sequence_number)
        .collect::<Vec<_>>()
}

fn assert_decoded_sequences(decoded: &[(LogstashEventFrame, usize)], expected_sequences: &[u32]) {
    assert_eq!(decoded_sequence_numbers(decoded), expected_sequences);
}

async fn assert_acknowledgements_for_ready_frames(
    decoded: Vec<(LogstashEventFrame, usize)>,
    expected_sequences: &[u32],
    expected_acknowledgements: &[u32],
) {
    assert_decoded_sequences(&decoded, expected_sequences);

    let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
    let mut ready = ReadyFrames::with_capacity(stream, 16);
    let (frames, _) = ready.next().await.unwrap().unwrap();

    // An incomplete window produces no ACK at all (`build_ack` returns
    // `None`); treat that as an empty acknowledgement list so callers can
    // assert it with `&[]`.
    let acknowledgements = LogstashAcker::new(&frames)
        .build_ack(TcpSourceAck::Ack)
        .map_or_else(Vec::new, decode_acknowledgements);

    assert!(ready.next().await.is_none());
    assert_eq!(acknowledgements, expected_acknowledgements);
}

fn decode_frames_and_assert_sequences(
    src: BytesMut,
    expected_sequences: &[u32],
) -> Vec<(LogstashEventFrame, usize)> {
    let decoded = decode_frames(src);
    assert_decoded_sequences(&decoded, expected_sequences);
    decoded
}

fn decode_frames_with_decoder(
    decoder: &mut LogstashDecoder,
    mut src: BytesMut,
) -> Vec<(LogstashEventFrame, usize)> {
    let mut frames = Vec::new();

    while let Some(frame) = decoder.decode(&mut src).unwrap() {
        frames.push(frame);
    }

    assert_eq!(src.len(), 0);
    frames
}

fn decode_frames_with_decoder_and_assert_sequences(
    decoder: &mut LogstashDecoder,
    src: BytesMut,
    expected_sequences: &[u32],
) -> Vec<(LogstashEventFrame, usize)> {
    let decoded = decode_frames_with_decoder(decoder, src);
    assert_decoded_sequences(&decoded, expected_sequences);
    decoded
}

#[test]
fn v1_decoder_does_not_panic() {
    let seq = rng().random_range(1..u32::MAX);
    let req = encode_req(seq, &[("message", "Hello, World!")]);
    for i in 0..req.len() - 1 {
        assert!(
            decode_data_frame(LogstashProtocolVersion::V1, &mut BytesMut::from(&req[..i]))
                .is_none()
        );
    }
}

// A malformed frame must be a fatal (non-continuable) decode error: the
// Lumberjack stream can't be resynced, so the connection is closed rather
// than continuing with a desynced decoder (which would emit bogus ACKs).
// This matches upstream logstash-input-beats, which closes the channel on
// any decode exception.

#[test]
fn malformed_json_frame_is_a_fatal_decode_error() {
    let mut decoder = LogstashDecoder::new();
    let mut src = BytesMut::new();
    src.put_u8(b'2');
    src.put_u8(b'J');
    src.put_u32(1); // sequence number
    let bad = b"{ not valid json ";
    src.put_u32(bad.len() as u32); // payload size
    src.put(&bad[..]);

    let err = decoder.decode(&mut src).unwrap_err();
    assert!(matches!(err, DecodeError::JsonFrameFailedDecode { .. }));
    assert!(
        !err.can_continue(),
        "a malformed JSON frame must be fatal so the connection closes",
    );
}

#[test]
fn malformed_compressed_frame_is_a_fatal_decode_error() {
    let mut decoder = LogstashDecoder::new();
    let mut src = BytesMut::new();
    src.put_u8(b'2');
    src.put_u8(b'C');
    let garbage = b"this is not a zlib stream";
    src.put_u32(garbage.len() as u32); // payload size
    src.put(&garbage[..]);

    let err = decoder.decode(&mut src).unwrap_err();
    assert!(matches!(err, DecodeError::DecompressionFailed { .. }));
    assert!(!err.can_continue());
}

#[test]
fn premature_window_size_frame_is_a_fatal_decode_error() {
    // A WindowSize frame that arrives before the current window has received
    // all its advertised events is a protocol violation, matching the
    // upstream Logstash/go-lumber server. The reference server's `readEvents`
    // loop reads exactly `window_size` frames and only accepts `J`/`D`/`C`
    // frame bytes inside that loop; any other byte — including a premature
    // `W` — hits its `default` branch and returns `ErrProtocolError`, closing
    // the connection (`go-lumber/server/v2/reader.go:121-124`). Every real
    // client (go-lumber, beats) sends exactly `window_size` events per window,
    // so a mid-window `WindowSize` means the sender has desynced; see the
    // "Window size" section of logstash.md. Rejecting it here means an
    // under-filled window can never reach the acker, so the acker never has to
    // invent a boundary for a window the sender closed early.
    let mut decoder = LogstashDecoder::new();
    let mut src = BytesMut::new();
    push_window_size(&mut src, 2);
    push_req(&mut src, 1, &[("message", "only one of two")]);
    push_window_size(&mut src, 5); // premature: window 1 still expects another event

    // The first decode yields the single data frame of the incomplete window.
    assert!(decoder.decode(&mut src).unwrap().is_some());

    // The next decode reaches the premature WindowSize and fails fatally so
    // the connection is closed rather than silently re-framed.
    let err = decoder.decode(&mut src).unwrap_err();
    assert!(matches!(err, DecodeError::PrematureWindowSize { .. }));
    assert!(
        !err.can_continue(),
        "a premature WindowSize must be fatal so the connection closes",
    );
}

#[test]
fn premature_window_size_inside_compressed_payload_is_fatal() {
    // The premature-WindowSize guard also fires inside a compressed payload.
    // Because the payload is expanded incrementally, the preceding valid frame
    // is delivered first and the error surfaces on the decode that reaches it.
    let mut inner = BytesMut::new();
    push_window_size(&mut inner, 2);
    push_req(&mut inner, 1, &[("message", "only one of two")]);
    push_window_size(&mut inner, 5); // premature, inside the compressed payload

    let mut req = BytesMut::new();
    push_compressed(&mut req, &inner);

    let mut decoder = LogstashDecoder::new();
    // The single valid frame of the incomplete window is delivered first.
    assert!(decoder.decode(&mut req).unwrap().is_some());
    // The next decode reaches the premature WindowSize and fails fatally.
    let err = decoder.decode(&mut req).unwrap_err();
    assert!(matches!(err, DecodeError::PrematureWindowSize { .. }));
    assert!(
        !err.can_continue(),
        "a premature WindowSize inside a compressed frame must be fatal",
    );
}

#[test]
fn compressed_frame_expansion_is_incremental() {
    // The reported OOM (H1 #3870642) came from expanding a whole compressed payload
    // into a frame list inside one `decode()` call, bypassing ReadyFrames batching.
    let mut inner = BytesMut::new();
    for seq in 1..=1000 {
        push_req(&mut inner, seq, &[("m", "")]);
    }

    let mut req = BytesMut::new();
    push_compressed(&mut req, &inner);

    let mut decoder = LogstashDecoder::new();
    let first = decoder.decode(&mut req).unwrap().unwrap();
    assert_eq!(first.0.sequence_number, 1);

    // After one decode on a 1000-frame payload the decoder must still be
    // mid-expansion, holding only the buffer and the nested decoder.
    assert!(
        matches!(
            decoder.state,
            LogstashDecoderReadState::PendingDecompressed { .. }
        ),
        "compressed expansion must be incremental, got {:?}",
        decoder.state
    );

    // And it must keep yielding one frame per call.
    let second = decoder.decode(&mut req).unwrap().unwrap();
    assert_eq!(second.0.sequence_number, 2);
}

#[test]
fn nested_compressed_frame_is_a_fatal_decode_error() {
    let mut inner = BytesMut::new();
    push_req(&mut inner, 1, &[("message", "should never be reached")]);

    let mut middle = BytesMut::new();
    push_compressed(&mut middle, &inner);

    let mut req = BytesMut::new();
    push_compressed(&mut req, &middle);

    let mut decoder = LogstashDecoder::new();
    let err = decoder.decode(&mut req).unwrap_err();
    assert!(matches!(err, DecodeError::NestedCompressedFrame));
    assert!(!err.can_continue());
}

#[test]
fn oversized_frames_are_rejected() {
    // A `2J` frame declares its payload size up front and is rejected on sight; a
    // `2D` frame declares only a pair count, so it is rejected once the bytes held
    // for the in-progress frame exceed the cap.
    let mut json = BytesMut::new();
    json.put_u8(b'2');
    json.put_u8(b'J');
    json.put_u32(1); // sequence number
    json.put_u32(100); // declared payload size, above the 8-byte cap

    let mut data = BytesMut::new();
    data.put_u8(b'2');
    data.put_u8(b'D');
    data.put_u32(1); // sequence number
    data.put_u32(u32::MAX); // absurd pair count
    data.put(&[0u8; 8][..]); // still incomplete, but past the 8-byte cap

    for (frame_type, mut src, expected_size) in [("json", json, 100), ("data", data, 16)] {
        let mut decoder = LogstashDecoder::new();
        decoder.max_frame_size = 8;

        let err = decoder.decode(&mut src).unwrap_err();
        assert!(
            matches!(err, DecodeError::FrameTooLarge { size, max: 8 } if size == expected_size),
            "{frame_type} frame: unexpected error {err:?}",
        );
        assert!(
            !err.can_continue(),
            "{frame_type} frame: an oversized frame must be fatal so the connection closes",
        );
    }
}

#[test]
fn frames_within_the_cap_still_decode() {
    let mut decoder = LogstashDecoder::new();
    decoder.max_frame_size = 100;

    let mut req = BytesMut::new();
    push_req(&mut req, 1, &[("message", "hello")]);

    let decoded = decode_frames_with_decoder(&mut decoder, req);
    assert_decoded_sequences(&decoded, &[1]);
}

#[test]
fn fragmented_input_is_assembled_across_decode_calls() {
    // Feed a frame one byte at a time to verify state is preserved across
    // `Ok(None)` returns (TCP can split at any byte boundary).
    let mut decoder = LogstashDecoder::new();
    let full = encode_req(7, &[("message", "hello")]);

    let mut src = BytesMut::new();
    let mut decoded = Vec::new();
    for byte in full.iter() {
        src.put_u8(*byte);
        if let Some(frame) = decoder.decode(&mut src).unwrap() {
            decoded.push(frame);
        }
    }
    assert_decoded_sequences(&decoded, &[7]);
}

#[tokio::test]
async fn malformed_frame_closes_connection_without_ack() {
    let (address, _recv) = start_logstash(EventStatus::Delivered).await;

    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();

    // A '2' 'J' frame whose payload is not valid JSON.
    let mut req = BytesMut::new();
    req.put_u8(b'2');
    req.put_u8(b'J');
    req.put_u32(1); // sequence number
    let bad = b"{ not valid json ";
    req.put_u32(bad.len() as u32); // payload size
    req.put(&bad[..]);
    socket.write_all(&req).await.unwrap();

    // The source must close the connection on the decode error and send no
    // ACK; the client will reconnect and retransmit.
    let mut output = BytesMut::new();
    let result = socket.read_buf(&mut output).await;
    assert!(
        matches!(result, Ok(0)) || result.is_err(),
        "expected the connection to close; read returned {result:?} with {output:?}",
    );
    assert!(
        output.is_empty(),
        "no ACK should be sent for a malformed frame, got {output:?}",
    );
}

#[tokio::test]
async fn distinct_windows_do_not_share_an_ack_domain() {
    let mut req = BytesMut::new();
    push_window_size(&mut req, 1);
    push_req(&mut req, 1, &[("message", "first window")]);
    push_window_size(&mut req, 2);
    push_req(&mut req, 1, &[("message", "second window first")]);
    push_req(&mut req, 2, &[("message", "second window second")]);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 1, 2]);
    assert_acknowledgements_for_ready_frames(decoded, &[1, 1, 2], &[1, 2]).await;
}

#[tokio::test]
async fn distinct_windows_with_monotonic_sequences_ack_the_first_window() {
    let mut req = BytesMut::new();
    push_window_size(&mut req, 2);
    push_req(&mut req, 1, &[("message", "first window first")]);
    push_req(&mut req, 2, &[("message", "first window second")]);
    push_window_size(&mut req, 2);
    push_req(&mut req, 3, &[("message", "second window first")]);
    push_req(&mut req, 4, &[("message", "second window second")]);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);
    assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 3, 4], &[2, 4]).await;
}

#[tokio::test]
async fn incomplete_window_is_not_acked() {
    // A window that has not yet received all its events must not be ACKed.
    // Emitting a partial ACK here puts a sequence number on the wire that
    // does not correspond to any window boundary the client declared; if
    // that ACK is later misattributed to a different (smaller) window by an
    // intermediary, the client rejects it as `invalid sequence number
    // received`. Matching the upstream Logstash server, we only ACK once
    // the window's final event arrives.
    let mut req = BytesMut::new();
    push_window_size(&mut req, 4);
    push_req(&mut req, 1, &[("message", "only event in partial window")]);

    let decoded = decode_frames_and_assert_sequences(req, &[1]);
    assert_acknowledgements_for_ready_frames(decoded, &[1], &[]).await;
}

#[tokio::test]
async fn window_split_across_compressed_frames_acks_once_on_completion() {
    // A single window whose advertised size exceeds the number of events in
    // any one compressed frame is split across several compressed frames. The
    // decoder must thread `window_events_remaining` out of each
    // `decode_compressed_frame` so the countdown continues across the
    // compression boundary: only the window's true final event is marked
    // `window_end`, yielding exactly one ACK. A threading bug would either
    // mark a spurious boundary inside an earlier compressed frame (two
    // window_ends -> two ACKs) or never complete the window at all.
    let mut first = BytesMut::new();
    push_req(&mut first, 1, &[("message", "w4 first")]);
    push_req(&mut first, 2, &[("message", "w4 second")]);

    let mut second = BytesMut::new();
    push_req(&mut second, 3, &[("message", "w4 third")]);
    push_req(&mut second, 4, &[("message", "w4 fourth")]);

    // WindowSize(4) is sent uncompressed (as beats does), then the four
    // events arrive two-per-compressed-frame, so each compressed frame
    // carries fewer events than the window size.
    let mut req = BytesMut::new();
    push_window_size(&mut req, 4);
    push_compressed(&mut req, &first);
    push_compressed(&mut req, &second);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);
    assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 3, 4], &[4]).await;
}

#[tokio::test]
async fn window_larger_than_ready_frames_capacity_in_one_compressed_frame_acks_once() {
    const WINDOW: u32 = 5;
    const CAPACITY: usize = 2;

    let mut inner = BytesMut::new();
    for seq in 1..=WINDOW {
        push_req(&mut inner, seq, &[("message", "event in oversized window")]);
    }

    // A following small window. Its sequence numbers restart at 1 (per the
    // protocol), and crucially its first event will share a ReadyFrames batch
    // with the oversized window's completing event (seq 5). That batch must
    // ACK only the oversized window (seq 5); the small window's retained first
    // event must NOT produce a second ACK until the small window's own final
    // event arrives.
    const SMALL_WINDOW: u32 = 2;
    let mut small_inner = BytesMut::new();
    for seq in 1..=SMALL_WINDOW {
        push_req(
            &mut small_inner,
            seq,
            &[("message", "event in small window")],
        );
    }

    // WindowSize is sent uncompressed (as beats does), then each whole window
    // arrives in a single compressed frame, exactly as in the report.
    let mut req = BytesMut::new();
    push_window_size(&mut req, WINDOW);
    push_compressed(&mut req, &inner);
    push_window_size(&mut req, SMALL_WINDOW);
    push_compressed(&mut req, &small_inner);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4, 5, 1, 2]);

    let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
    let mut ready = ReadyFrames::with_capacity(stream, CAPACITY);
    let mut acknowledgements = Vec::new();

    while let Some(result) = ready.next().await {
        let (frames, _byte_size) = result.unwrap();
        let acks = LogstashAcker::new(&frames)
            .build_ack(TcpSourceAck::Ack)
            .map_or_else(Vec::new, decode_acknowledgements);
        acknowledgements.push(acks);
    }

    // Batches: [1, 2] [3, 4] [5, 1] [2].
    // - The oversized window completes in the [5, 1] batch -> ACK(5) only; the
    //   retained small-window seq 1 in that same batch is NOT a window end, so
    //   it produces no ACK.
    // - The small window completes in the [2] batch -> ACK(2).
    // Each window is therefore ACKed exactly once; the oversized window's
    // sequence number (5) never reappears in a later batch.
    assert_eq!(acknowledgements, vec![vec![], vec![], vec![5], vec![2]]);

    // Each window is ACKed exactly once: seq 5 (oversized) and seq 2 (small).
    let all_acks: Vec<u32> = acknowledgements.into_iter().flatten().collect();
    assert_eq!(
        all_acks,
        vec![5, 2],
        "each window must be ACKed exactly once"
    );
}

#[tokio::test]
async fn complete_window_then_incomplete_window_acks_only_the_complete_one() {
    // The customer-reported failure: a small complete window followed by a
    // larger window that is only partially present in the same batch. The
    // acker must emit only the completed window's ACK and must not emit a
    // partial ACK for the incomplete trailing window, whose sequence number
    // would otherwise exceed the smaller window the client awaits.
    let mut req = BytesMut::new();
    push_window_size(&mut req, 2);
    push_req(&mut req, 1, &[("message", "complete window first")]);
    push_req(&mut req, 2, &[("message", "complete window second")]);
    push_window_size(&mut req, 1000);
    push_req(&mut req, 1, &[("message", "partial window first")]);
    push_req(&mut req, 2, &[("message", "partial window second")]);
    push_req(&mut req, 3, &[("message", "partial window third")]);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 1, 2, 3]);
    assert_acknowledgements_for_ready_frames(decoded, &[1, 2, 1, 2, 3], &[2]).await;
}

#[tokio::test]
async fn compressed_frames_preserve_inner_window_boundaries() {
    let mut inner = BytesMut::new();
    push_window_size(&mut inner, 2);
    push_req(&mut inner, 1, &[("message", "compressed first")]);
    push_req(&mut inner, 2, &[("message", "compressed second")]);

    let mut req = BytesMut::new();
    push_compressed(&mut req, &inner);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2]);
    assert_acknowledgements_for_ready_frames(decoded, &[1, 2], &[2]).await;
}

#[tokio::test]
async fn single_window_split_across_ready_frames_acks_only_on_completion() {
    // When one writer window is split across multiple `ReadyFrames` batches,
    // only the batch containing the window's final event (its `window_end`)
    // produces an ACK. Earlier batches hold no window boundary, so they emit
    // nothing rather than a partial ACK for a mid-window sequence number.
    let mut req = BytesMut::new();
    push_window_size(&mut req, 4);
    push_req(&mut req, 1, &[("message", "first")]);
    push_req(&mut req, 2, &[("message", "second")]);
    push_req(&mut req, 3, &[("message", "third")]);
    push_req(&mut req, 4, &[("message", "fourth")]);

    let decoded = decode_frames_and_assert_sequences(req, &[1, 2, 3, 4]);

    let stream = stream::iter(decoded.into_iter().map(Ok::<_, DecodeError>));
    let mut ready = ReadyFrames::with_capacity(stream, 2);
    let mut acknowledgements = Vec::new();

    while let Some(result) = ready.next().await {
        let (frames, _byte_size) = result.unwrap();
        let acks = LogstashAcker::new(&frames)
            .build_ack(TcpSourceAck::Ack)
            .map_or_else(Vec::new, decode_acknowledgements);
        acknowledgements.push(acks);
    }

    // First batch (seq 1, 2) holds no window boundary -> no ACK.
    // Second batch (seq 3, 4) completes the window -> ACK(4).
    assert_eq!(acknowledgements, vec![vec![], vec![4]]);
}

#[tokio::test]
async fn fresh_window_after_completed_window_is_accepted() {
    // A decoder reused across reads accepts a fresh window once the previous
    // window has completed. This is the legitimate counterpart to the
    // premature-WindowSize error: a `WindowSize` is only rejected when it
    // arrives mid-window, so opening a new window after the prior one has
    // received all its advertised events is always allowed. (A `WindowSize`
    // after a still-partial window would now be a fatal protocol error; see
    // `premature_window_size_frame_is_a_fatal_decode_error`.)
    let mut decoder = LogstashDecoder::new();

    let mut first_batch = BytesMut::new();
    push_window_size(&mut first_batch, 1);
    push_req(&mut first_batch, 1, &[("message", "first window")]);
    let decoded = decode_frames_with_decoder_and_assert_sequences(&mut decoder, first_batch, &[1]);
    assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;

    let mut second_batch = BytesMut::new();
    push_window_size(&mut second_batch, 1);
    push_req(
        &mut second_batch,
        1,
        &[("message", "fresh window after completion")],
    );
    let decoded = decode_frames_with_decoder_and_assert_sequences(&mut decoder, second_batch, &[1]);
    assert_acknowledgements_for_ready_frames(decoded, &[1], &[1]).await;
}

async fn send_req(address: SocketAddr, pairs: &[(&str, &str)], sends_ack: bool) {
    let seq = rng().random_range(1..u32::MAX);
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();

    let req = encode_req(seq, pairs);
    socket.write_all(&req).await.unwrap();

    let mut output = BytesMut::new();
    socket.read_buf(&mut output).await.unwrap();

    if sends_ack {
        assert_eq!(output.get_u8(), b'2');
        assert_eq!(output.get_u8(), b'A');
        assert_eq!(output.get_u32(), seq);
    }
    assert_eq!(output.len(), 0);
}

#[test]
fn output_schema_definition_vector_namespace() {
    let config = LogstashConfig {
        log_namespace: Some(true),
        ..Default::default()
    };

    let definitions = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    let expected_definition =
        Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
            .with_meaning(OwnedTargetPath::event_root(), "message")
            .with_metadata_field(
                &owned_value_path!("vector", "source_type"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("vector", "ingest_timestamp"),
                Kind::timestamp(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(LogstashConfig::NAME, "timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_metadata_field(
                &owned_value_path!(LogstashConfig::NAME, "host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_metadata_field(
                &owned_value_path!(LogstashConfig::NAME, "tls_client_metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

    assert_eq!(definitions, Some(expected_definition))
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let config = LogstashConfig::default();

    let definitions = config
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()),
        [LogNamespace::Legacy],
    )
    .with_event_field(
        &owned_value_path!("message"),
        Kind::bytes(),
        Some("message"),
    )
    .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
    .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"));

    assert_eq!(definitions, Some(expected_definition))
}
