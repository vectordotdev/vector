use crate::{
    async_read::AsyncAllowReadExt, emit, event::Event, internal_events::UnixSocketError,
    shutdown::ShutdownSignal, sources::Source, stream::StreamExt,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use std::path::PathBuf;
use tokio01::{
    self,
    codec::{FramedRead, LinesCodec},
};
use tokio_uds::UnixListener;
use tracing::field;
use tracing_futures::Instrument;

/**
* Returns a Source object corresponding to a Unix domain socket.  Passing in different functions
* for build_event can allow for different source-specific logic (such as decoding syslog messages
* in the syslog source).
**/
pub fn build_unix_source(
    path: PathBuf,
    max_length: usize,
    host_key: String,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
    build_event: impl Fn(&str, Option<Bytes>, &str) -> Option<Event>
        + std::marker::Send
        + std::marker::Sync
        + std::clone::Clone
        + 'static,
) -> Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");

        info!(message = "listening.", ?path, r#type = "unix");

        listener
            .incoming()
            .take_until(shutdown.clone())
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();
                let peer_addr = socket.peer_addr().ok();
                let host_key = host_key.clone();
                let listen_path = path.clone();

                let span = info_span!("connection");
                let path = if let Some(addr) = peer_addr {
                    if let Some(path) = addr.as_pathname().map(|e| e.to_owned()) {
                        span.record("peer_path", &field::debug(&path));
                        Some(path)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let build_event = build_event.clone();
                let received_from: Option<Bytes> =
                    path.map(|p| p.to_string_lossy().into_owned().into());
                let lines_in = FramedRead::new(
                    socket.allow_read_until(shutdown.clone()),
                    LinesCodec::new_with_max_length(max_length),
                )
                .filter_map(move |line| build_event(&host_key, received_from.clone(), &line))
                .map_err(move |error| {
                    emit!(UnixSocketError {
                        error,
                        path: &listen_path,
                    });
                });

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));
                tokio01::spawn(handler.instrument(span))
            })
    }))
}
