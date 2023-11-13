use std::io;

use async_stream::stream;
use bytes::Bytes;
use chrono::Utc;
use futures::{channel::mpsc, executor, SinkExt, StreamExt};
use tokio_util::{codec::FramedRead, io::StreamReader};
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::configurable::NamedComponent;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol,
};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    event::Event,
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::Kind;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{log_schema, SourceOutput},
    internal_events::{EventsReceived, FileDescriptorReadError, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};

#[cfg(all(unix, feature = "sources-file-descriptor"))]
pub mod file_descriptor;
#[cfg(feature = "sources-stdin")]
pub mod stdin;

pub trait FileDescriptorConfig: NamedComponent {
    fn host_key(&self) -> Option<OptionalValuePath>;
    fn framing(&self) -> Option<FramingConfig>;
    fn decoding(&self) -> DeserializerConfig;
    fn description(&self) -> String;

    fn source<R>(
        &self,
        reader: R,
        shutdown: ShutdownSignal,
        out: SourceSender,
        log_namespace: LogNamespace,
    ) -> crate::Result<crate::sources::Source>
    where
        R: Send + io::BufRead + 'static,
    {
        let host_key = self
            .host_key()
            .and_then(|k| k.path)
            .or(log_schema().host_key().cloned());
        let hostname = crate::get_hostname().ok();

        let description = self.description();

        let decoding = self.decoding();
        let framing = self
            .framing()
            .unwrap_or_else(|| decoding.default_stream_framing());
        let decoder = DecodingConfig::new(framing, decoding, log_namespace).build()?;

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
            self.get_component_name(),
            hostname,
            log_namespace,
        )))
    }
}

type Sender = mpsc::Sender<Result<Bytes, io::Error>>;

fn read_from_fd<R>(mut reader: R, mut sender: Sender)
where
    R: Send + io::BufRead + 'static,
{
    loop {
        let (buffer, len) = match reader.fill_buf() {
            Ok(buffer) if buffer.is_empty() => break, // EOF.
            Ok(buffer) => (Ok(Bytes::copy_from_slice(buffer)), buffer.len()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => (Err(error), 0),
        };

        reader.consume(len);

        if executor::block_on(sender.send(buffer)).is_err() {
            // Receiver has closed so we should shutdown.
            break;
        }
    }
}

type Receiver = mpsc::Receiver<Result<Bytes, io::Error>>;

#[allow(clippy::too_many_arguments)]
async fn process_stream(
    receiver: Receiver,
    decoder: Decoder,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
    host_key: Option<OwnedValuePath>,
    source_type: &'static str,
    hostname: Option<String>,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let bytes_received = register!(BytesReceived::from(Protocol::NONE));
    let events_received = register!(EventsReceived);
    let stream = receiver.inspect(|result| {
        if let Err(error) = result {
            emit!(FileDescriptorReadError { error: &error });
        }
    });
    let stream = StreamReader::new(stream);
    let mut stream = FramedRead::new(stream, decoder).take_until(shutdown);
    let mut stream = stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok((events, byte_size)) => {
                    bytes_received.emit(ByteSize(byte_size));
                    events_received.emit(CountByteSize(
                         events.len(),
                         events.estimated_json_encoded_size_of(),
                    ));

                    let now = Utc::now();

                    for mut event in events {
                        match event{
                            Event::Log(_) => {
                                let log = event.as_mut_log();

                                log_namespace.insert_standard_vector_source_metadata(
                                    log,
                                    source_type,
                                    now
                                );

                                if let Some(hostname) = &hostname {
                                    log_namespace.insert_source_metadata(
                                        source_type,
                                        log,
                                        host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                        path!("host"),
                                        hostname.clone()
                                    );
                                }

                                yield event;
                            },
                            _ => {
                                yield event;
                            }
                        }
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
        Err(_) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { count });
            Err(())
        }
    }
}

/// Builds the `vector_lib::config::Outputs` for stdin and
/// file_descriptor sources.
fn outputs(
    log_namespace: LogNamespace,
    host_key: &Option<OptionalValuePath>,
    decoding: &DeserializerConfig,
    source_name: &'static str,
) -> Vec<SourceOutput> {
    let schema_definition = decoding
        .schema_definition(log_namespace)
        .with_source_metadata(
            source_name,
            host_key
                .clone()
                .map_or(log_schema().host_key().cloned(), |key| key.path)
                .map(LegacyKey::Overwrite),
            &owned_value_path!("host"),
            Kind::bytes(),
            Some("host"),
        )
        .with_standard_vector_source_metadata();

    vec![SourceOutput::new_logs(
        decoding.output_type(),
        schema_definition,
    )]
}
