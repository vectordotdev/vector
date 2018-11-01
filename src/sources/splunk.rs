use futures::{future, sync::mpsc, Future, Sink, Stream};
use log::{error, info};
use std::net::SocketAddr;
use stream_cancel::{StreamExt, Tripwire};
use tokio::{
    self,
    codec::{FramedRead, LinesCodec},
    net::TcpListener,
};

pub fn raw_tcp(addr: SocketAddr, exit: Tripwire) -> impl Stream<Item = String, Error = ()> {
    // TODO: buf size?
    let (tx, rx) = mpsc::channel(1000);
    let listener = TcpListener::bind(&addr).expect("failed to bind to listener socket");

    info!("listening on {:?}", listener.local_addr());

    let server = listener
        .incoming()
        .take_until(exit)
        .map_err(|e| error!("failed to accept socket; error = {:?}", e))
        .for_each(move |socket| {
            let tx = tx.clone();

            let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(100 * 1024))
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
