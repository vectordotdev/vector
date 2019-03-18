use clap::{App, Arg};
use futures::{Future, Stream};
use tokio_signal::unix::{Signal, SIGINT, SIGQUIT, SIGTERM};
use trace_metrics::MetricsSubscriber;
use vector::metrics;
use vector::topology::Topology;

#[macro_use]
extern crate tokio_trace;

fn main() {
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
                .help("Causes vector to immediate exit on startup if any sinks having failing healthchecks")
        );
    let matches = app.get_matches();

    let config = matches.value_of("config").unwrap();

    let config = vector::topology::Config::load(std::fs::File::open(config).unwrap());

    let (metrics_sink, metrics_server) = metrics::metrics();

    let subscriber = tokio_trace_fmt::FmtSubscriber::builder()
        .with_filter(tokio_trace_fmt::filter::EnvFilter::from(
            "vector=info,vector[sink]=info",
        ))
        .full()
        .finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");

    let subscriber = MetricsSubscriber::new(subscriber, metrics_sink);

    tokio_trace::subscriber::with_default(subscriber, || {
        let topology = config.and_then(Topology::build);

        let mut topology = match topology {
            Ok((topology, warnings)) => {
                for warning in warnings {
                    error!("Configuration warning: {}", warning);
                }

                topology
            }
            Err(errors) => {
                for error in errors {
                    error!("Configuration error: {}", error);
                }
                return;
            }
        };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let metrics_addr = "127.0.0.1:8888".parse().unwrap();
        let metrics_serve = metrics::serve(metrics_addr, metrics_server);

        rt.spawn(metrics_serve);

        if matches.is_present("require-healthy") {
            let success = rt.block_on(topology.healthchecks());

            if success.is_ok() {
                info!("All healthchecks passed");
            } else {
                error!("Sinks unhealthy; shutting down");
                std::process::exit(1);
            }
        } else {
            rt.spawn(topology.healthchecks());
        }

        topology.start(&mut rt);

        let sigint = Signal::new(SIGINT).flatten_stream();
        let sigterm = Signal::new(SIGTERM).flatten_stream();
        let sigquit = Signal::new(SIGQUIT).flatten_stream();

        let signals = sigint.select(sigterm.select(sigquit));

        let (signal, signals) = rt.block_on(signals.into_future()).ok().unwrap();
        let signal = signal.unwrap();

        if signal == SIGINT || signal == SIGTERM {
            use futures::future::Either;

            info!("Shutting down");
            topology.stop();

            let shutdown = rt.shutdown_on_idle();

            match shutdown.select2(signals.into_future()).wait() {
                Ok(Either::A(_)) => { /* Graceful shutdown finished */ }
                Ok(Either::B(_)) => {
                    info!("Shutting down immediately");
                    // Dropping the shutdown future will immediately shut the server down
                }
                Err(_) => unreachable!(),
            }
        } else if signal == SIGQUIT {
            info!("Shutting down immediately");
            rt.shutdown_now().wait().unwrap();
        } else {
            unreachable!();
        }
    });
}
