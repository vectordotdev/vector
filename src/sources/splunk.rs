use futures::{future, sync::mpsc, Future, Sink, Stream};
use std::net::SocketAddr;
use tokio::{
    self,
    codec::{Decoder, LinesCodec},
    net::TcpListener,
};

pub fn raw_tcp(addr: SocketAddr) -> impl Stream<Item = String, Error = ()> {
    // TODO: buf size?
    let (tx, rx) = mpsc::channel(0);
    let server = TcpListener::bind(&addr)
        .expect("failed to bind to listener socket")
        .incoming()
        .map_err(|e| error!("failed to accept socket; error = {:?}", e))
        .for_each(move |socket| {
            let tx = tx.clone();

            let lines_in = LinesCodec::new_with_max_length(100 * 1024)
                .framed(socket)
                // .map(|s| Bytes::from(s))
                .map_err(|e| error!("error reading line: {:?}", e));

            let handler = tx
                .sink_map_err(|e| error!("error sending line: {:?}", e))
                .send_all(lines_in)
                .map(|_| info!("finished sending"));

            tokio::spawn(handler)
        });

    future::lazy(move || tokio::spawn(server))
        .map(|_| String::new())
        .into_stream()
        .chain(rx)
        .skip(1)
}
