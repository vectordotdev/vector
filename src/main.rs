use clap::{App, Arg};
use futures::{Future, Stream};
use log::info;
use router::topology;
use tokio_signal::unix::{Signal, SIGINT, SIGQUIT, SIGTERM};

fn main() {
    router::setup_logger();

    let app = App::new("Router").version("1.0").author("timber.io").arg(
        Arg::with_name("config")
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file")
            .required(true)
            .takes_value(true),
    );
    let matches = app.get_matches();

    let config = matches.value_of("config").unwrap();
    let config: router::topology::Config =
        serde_json::from_reader(std::fs::File::open(config).unwrap()).unwrap();

    let (server, server_trigger) = topology::build(config);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);

    let signals = vec![SIGINT, SIGTERM, SIGQUIT]
        .into_iter()
        .map(|sig| Signal::new(sig).flatten_stream().into_future());
    let signals = futures::future::select_ok(signals);

    let (signal, _) = rt.block_on(signals).ok().unwrap();
    let signal = signal.0.unwrap();

    if signal == SIGINT || signal == SIGTERM {
        info!("Shutting down");
        drop(server_trigger);
        rt.shutdown_on_idle().wait().unwrap();
    } else if signal == SIGQUIT {
        info!("Shutting down immediately");
        rt.shutdown_now().wait().unwrap();
    } else {
        unreachable!();
    }
}
