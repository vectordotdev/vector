use crate::record::Record;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::{
    self,
    codec::{FramedRead, LinesCodec},
    net::TcpListener,
};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    pub address: std::net::SocketAddr,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
}

fn default_max_length() -> usize {
    100 * 1024
}

impl TcpConfig {
    pub fn new(addr: std::net::SocketAddr) -> Self {
        Self {
            address: addr,
            max_length: default_max_length(),
        }
    }
}

#[typetag::serde(name = "tcp")]
impl crate::topology::config::SourceConfig for TcpConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        Ok(tcp(self.address, self.max_length, out))
    }
}

pub fn tcp(addr: SocketAddr, max_length: usize, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = TcpListener::bind(&addr).expect("failed to bind to listener socket");

        info!("listening on {:?}", listener.local_addr());

        listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let host = socket.peer_addr().ok().map(|s| s.ip().to_string());

                let out = out.clone();

                let lines_in = FramedRead::new(socket, LinesCodec::new_with_max_length(max_length))
                    .map(Record::new_from_line)
                    .map(move |mut record| {
                        record.host = host.clone();
                        record
                    })
                    .map_err(|e| error!("error reading line: {:?}", e));

                let handler = lines_in.forward(out).map(|_| info!("finished sending"));

                tokio::spawn(handler)
            })
    }))
}

#[cfg(test)]
mod test {
    use crate::test_util::{next_addr, send_lines, wait_for_tcp};
    use futures::sync::mpsc;
    use futures::Stream;

    #[test]
    fn it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = super::tcp(addr, super::default_max_length(), tx);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let record = rx.wait().next().unwrap().unwrap();
        assert_eq!(record.host, Some("127.0.0.1".to_owned()));
    }

    #[test]
    fn it_defaults_max_length() {
        let with: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            max_length = 19
            "#,
        )
        .unwrap();

        let without: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            "#,
        )
        .unwrap();

        assert_eq!(with.max_length, 19);
        assert_eq!(without.max_length, super::default_max_length());
    }
}
