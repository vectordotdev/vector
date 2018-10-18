use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::channel,
    Arc,
};
use std::thread;

use transport::{Consumer, Log};

pub struct RawTcpSource {
    log: Log,
}

impl RawTcpSource {
    pub fn new(log: Log) -> Self {
        RawTcpSource { log }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        thread::spawn(move || {
            // TODO: more efficient way to handle multiple writers?
            let (tx, rx) = channel();

            let listener = TcpListener::bind("0.0.0.0:1234").expect("failed to bind to tcp socket");
            let listener_handle = thread::spawn(move || {
                // only taking 1 connection for now so we don't run forever
                for stream in listener.incoming().take(1) {
                    let tx = tx.clone();
                    let conn = stream.expect("failed to open tcpstream");
                    // connection handling thread
                    thread::spawn(move || {
                        let reader = BufReader::new(conn);
                        for line in reader.lines().filter_map(Result::ok) {
                            tx.send(line).expect("failed to send line to writer");
                        }
                    });
                }
            });

            let writer_handle = thread::spawn(move || {
                let mut offset = 0;
                for line in rx.iter() {
                    self.log
                        .append(&[line.as_bytes()])
                        .expect("failed to append line");
                    offset += 1;
                }
                offset
            });

            listener_handle
                .join()
                .expect("failed to join listener thread");
            writer_handle.join().expect("failed to join writer thread")
        })
    }
}

pub struct RawTcpSink {
    consumer: Consumer,
    stream: TcpStream,
    last_offset: Arc<AtomicUsize>,
}

impl RawTcpSink {
    pub fn new(
        consumer: Consumer,
        addr: impl ToSocketAddrs,
        last_offset: Arc<AtomicUsize>,
    ) -> Self {
        let stream = TcpStream::connect(addr).unwrap();
        RawTcpSink {
            consumer,
            stream,
            last_offset,
        }
    }

    pub fn run(mut self) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut offset = 0;
            let mut writer = BufWriter::new(self.stream);
            while let Ok(batch) = self.consumer.poll() {
                if batch.is_empty() {
                    let lo = self.last_offset.load(Ordering::Relaxed);
                    if lo > 0 && offset == lo {
                        break;
                    }
                } else {
                    for record in batch {
                        writer.write_all(&record).unwrap();
                        writer.write_all(b"\n").unwrap();
                        offset += 1;
                    }
                }
            }
        })
    }
}
