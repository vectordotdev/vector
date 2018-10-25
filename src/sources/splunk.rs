use futures::{sync::mpsc, Future, Sink, Stream};
use std::fmt::Debug;
use std::io::BufWriter;
use std::net::SocketAddr;
use tokio::{
    self,
    codec::{Decoder, FramedWrite, LinesCodec},
    fs::File,
    net::TcpListener,
};

pub fn raw_tcp(addr: SocketAddr) -> impl Stream<Item = String, Error = ()> {
    let (tx, rx) = mpsc::channel(1000);
    let listener = TcpListener::bind(&addr).unwrap();

    let server = TcpListener::bind(&addr)
        .unwrap()
        .incoming()
        .map_err(|e| error!("failed to accept socket; error = {:?}", e))
        .for_each(move |socket| {
            let tx = tx.clone();

            let lines_in = LinesCodec::new_with_max_length(100 * 1024)
                .framed(socket)
                // .map(|s| Bytes::from(s))
                .map_err(|e| error!("error reading line: {:?}", e));

            let handler = tx
                .sink_map_err(|e| error!("error sending lines: {:?}", e))
                .send_all(lines_in)
                .map(|_| info!("finished sending"));

            tokio::spawn(handler)
        });

    rx
}
