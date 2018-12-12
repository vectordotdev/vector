use crate::record::Record;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use log::{error, info};
use std::net::SocketAddr;
use tokio::{
    self,
    codec::{FramedRead, LinesCodec},
    net::TcpListener,
};

pub fn raw_tcp(addr: SocketAddr, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = TcpListener::bind(&addr).expect("failed to bind to listener socket");

        info!("listening on {:?}", listener.local_addr());

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(100 * 1024))
                    .map(Record::new_from_line)
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler)
            })
    }))
}
