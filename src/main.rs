use clap::{App, Arg};
use router::topology;

fn main() {
    // router::setup_logger();

    let app = App::new("Router").version("1.0").author("timber.io")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .required(true)
                .takes_value(true),
        ).arg(
            Arg::with_name("require-healthy")
                .short("r")
                .long("require-healthy")
                .help("Causes router to immediate exit on startup if any sinks having failing healthchecks")
        );
    let matches = app.get_matches();

    let config = matches.value_of("config").unwrap();

    let config = router::topology::Config::load(std::fs::File::open(config).unwrap());

    let topology = config.and_then(topology::build);

    let subscriber = tokio_trace_fmt::FmtSubscriber::builder().full().finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");

    tokio_trace::subscriber::with_default(subscriber, || {
        router::run_server(topology, matches.is_present("require-healthy"));
    });
}
