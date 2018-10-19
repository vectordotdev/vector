extern crate router;

#[macro_use]
extern crate log;

#[macro_use]
extern crate duct;
extern crate os_pipe;

use std::io::{BufRead, BufReader};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    router::setup_logger();

    info!("starting test harness");

    let (reader, writer) = os_pipe::pipe().unwrap();
    let receiver_handle = cmd!("nc", "-l", "9999")
        .stdout_handle(writer)
        .start()
        .unwrap();

    info!("starting server");
    let server_handle = cmd!("cargo", "run", "--release", "--bin", "router")
        .start()
        .unwrap();
    thread::sleep(Duration::from_millis(1000));
    assert!(server_handle.try_wait().unwrap().is_none());

    let input = cmd!("echo", "important: i am first")
        .then(cmd!("flog", "-b", format!("{}", 10 * 1024 * 1024)))
        .then(cmd!("echo", "important: i am last"))
        .pipe(cmd!("cat")); // why is this necessary?
    let sender = cmd!("nc", "localhost", "1234");

    info!("starting test");
    let start = Instant::now();
    let input_handle = input.pipe(sender).start().unwrap();

    info!("starting validation");
    for line in BufReader::new(reader).lines().filter_map(Result::ok) {
        if line.contains("important") {
            info!("{}: {:?}", line, start.elapsed());
        }
    }

    info!("done validating");
    input_handle.wait().unwrap();
    server_handle.kill().unwrap();
    receiver_handle.wait().unwrap();
}
