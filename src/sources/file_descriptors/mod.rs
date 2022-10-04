use std::io;

use async_stream::stream;
use bytes::Bytes;
use chrono::Utc;
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{channel::mpsc, executor, SinkExt, StreamExt};
use tokio_util::{codec::FramedRead, io::StreamReader};
use vector_common::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_config::NamedComponent;
use vector_core::config::LogNamespace;
use vector_core::ByteSizeOf;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::log_schema,
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};

#[cfg(all(unix, feature = "sources-file-descriptor"))]
pub mod file_descriptor;
#[cfg(feature = "sources-stdin")]
pub mod stdin;

pub trait FileDescriptorConfig: NamedComponent {
    fn host_key(&self) -> Option<String>;
    fn framing(&self) -> Option<FramingConfig>;
    fn decoding(&self) -> DeserializerConfig;
    fn description(&self) -> String;

    fn source<R>(
        &self,
        reader: R,
        shutdown: ShutdownSignal,
        out: SourceSender,
    ) -> crate::Result<crate::sources::Source>
    where
        R: Send + io::BufRead + 'static,
    {
        let host_key = self
            .host_key()
            .unwrap_or_else(|| log_schema().host_key().to_string());
        let hostname = crate::get_hostname().ok();

        let source_type = Bytes::from_static(Self::NAME.as_bytes());
        let description = self.description();

        let decoding = self.decoding();
        let framing = self
            .framing()
            .unwrap_or_else(|| decoding.default_stream_framing());
        let decoder = DecodingConfig::new(framing, decoding, LogNamespace::Legacy).build();

        let (sender, receiver) = mpsc::channel(1024);

        // Spawn background thread with blocking I/O to process fd.
        //
        // This is recommended by Tokio, as otherwise the process will not shut down
        // until another newline is entered. See
        // https://github.com/tokio-rs/tokio/blob/a73428252b08bf1436f12e76287acbc4600ca0e5/tokio/src/io/stdin.rs#L33-L42
        std::thread::spawn(move || {
            info!("Capturing {}.", description);
            read_from_fd(reader, sender);
        });

        Ok(Box::pin(process_stream(
            receiver,
            decoder,
            out,
            shutdown,
            host_key,
            source_type,
            hostname,
        )))
    }
}

type Sender = mpsc::Sender<std::result::Result<bytes::Bytes, std::io::Error>>;
fn read_from_fd<R>(mut reader: R, mut sender: Sender)
where
    R: Send + io::BufRead + 'static,
{
    loop {
        let (buffer, len) = match reader.fill_buf() {
            Ok(buffer) if buffer.is_empty() => break, // EOF.
            Ok(buffer) => (Ok(Bytes::copy_from_slice(buffer)), buffer.len()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => (Err(error), 0),
        };

        reader.consume(len);

        if executor::block_on(sender.send(buffer)).is_err() {
            // Receiver has closed so we should shutdown.
            break;
        }
    }
}

type Receiver = mpsc::Receiver<std::result::Result<bytes::Bytes, std::io::Error>>;

async fn process_stream(
    receiver: Receiver,
    decoder: Decoder,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
    host_key: String,
    source_type: Bytes,
    hostname: Option<String>,
) -> Result<(), ()> {
    let bytes_received = register!(BytesReceived::from(Protocol::NONE));
    let stream = StreamReader::new(receiver);
    let mut stream = FramedRead::new(stream, decoder).take_until(shutdown);
    let mut stream = stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok((events, byte_size)) => {
                    bytes_received.emit(ByteSize(byte_size));
                    emit!(EventsReceived {
                        byte_size: events.size_of(),
                        count: events.len()
                    });

                    let now = Utc::now();

                    for mut event in events {
                        let log = event.as_mut_log();

                        log.try_insert(log_schema().source_type_key(), source_type.clone());
                        log.try_insert(log_schema().timestamp_key(), now);

                        if let Some(hostname) = &hostname {
                            log.try_insert(host_key.as_str(), hostname.clone());
                        }

                        yield event;
                    }
                }
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no
                    // further handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }
    .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(error) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { error, count });
            Err(())
        }
    }
}
