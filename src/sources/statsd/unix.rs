use vector_lib::codecs::{
    NewlineDelimitedDecoder,
    decoding::{Deserializer, Framer},
};

use super::{StatsdDeserializer, UnixConfig, UnixMode};
use crate::{
    SourceSender,
    codecs::Decoder,
    shutdown::ShutdownSignal,
    sources::{
        Source,
        util::{build_unix_datagram_source, build_unix_stream_source},
    },
};

pub fn statsd_unix(
    config: UnixConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let decoder = Decoder::new(
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
        Deserializer::Boxed(Box::new(StatsdDeserializer::unix(
            config.sanitize,
            config.convert_to,
        ))),
    );

    match config.unix_mode {
        UnixMode::Stream => build_unix_stream_source(
            config.path,
            config.socket_file_mode,
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
        UnixMode::Datagram => build_unix_datagram_source(
            config.path,
            config.socket_file_mode,
            crate::serde::default_max_length(),
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
    }
}
