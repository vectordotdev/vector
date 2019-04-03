use clap::{App, Arg};
use futures::{future, Future, Stream};
use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use trace_metrics::MetricsSubscriber;
use vector::metrics;
use vector::topology::Topology;

#[macro_use]
extern crate tokio_trace;

fn main() {
    let app = App::new("Vector").version("1.0").author("timber.io")
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
        )
        .arg(
            Arg::with_name("metrics-addr")
                .short("m")
                .long("metrics-addr")
                .default_value("127.0.0.1:8888")
                .help("The address that metrics will be served from")
        );
    let matches = app.get_matches();

    let config_path = matches.value_of("config").unwrap();

    let addr = matches.value_of("metrics-addr").unwrap();
    let metrics_addr = if let Ok(addr) = addr.parse() {
        addr
    } else {
        error!("Unable to parse metrics address {}", addr);
        std::process::exit(1);
    };

    let (metrics_sink, metrics_server) = metrics::build(&metrics_addr);

    let subscriber = tokio_trace_fmt::FmtSubscriber::builder()
        .with_filter(tokio_trace_fmt::filter::EnvFilter::from(
            "vector=info,vector[sink]=info",
        ))
        .full()
        .finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");

    let subscriber = MetricsSubscriber::new(subscriber, metrics_sink);

    tokio_trace::subscriber::with_default(subscriber, || {
        let file = match std::fs::File::open(config_path) {
            Ok(f) => f,
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    error!("Config file not found in: {}", config_path);
                    std::process::exit(1);
                } else {
                    panic!("Error opening config file: {}", e)
                }
            }
        };
        let config = vector::topology::Config::load(file);

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

        let (metrics_trigger, metrics_tripwire) = stream_cancel::Tripwire::new();

        rt.spawn(
            metrics_server
                .select(metrics_tripwire)
                .map(|_| ())
                .map_err(|_| ()),
        );

        let require_healthy = matches.is_present("require-healthy");

        if require_healthy {
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

        let mut graceful_crash = topology.start(&mut rt);

        let sigint = Signal::new(SIGINT).flatten_stream();
        let sigterm = Signal::new(SIGTERM).flatten_stream();
        let sigquit = Signal::new(SIGQUIT).flatten_stream();
        let sighup = Signal::new(SIGHUP).flatten_stream();

        let mut signals = sigint.select(sigterm.select(sigquit.select(sighup)));

        let signal = loop {
            let signal = future::poll_fn(|| signals.poll());
            let crash = future::poll_fn(|| graceful_crash.poll());

            let next = signal
                .select2(crash)
                .wait()
                .map_err(|_| ())
                .expect("Neither stream errors");

            let signal = match next {
                future::Either::A((signal, _)) => signal.expect("Signal streams never end"),
                future::Either::B((_crash, _)) => SIGINT, // Trigger graceful shutdown if a component crashed
            };

            if signal != SIGHUP {
                break signal;
            }

            // Reload config
            let file = match std::fs::File::open(config_path) {
                Ok(f) => f,
                Err(e) => {
                    if let std::io::ErrorKind::NotFound = e.kind() {
                        error!("Config file not found in: {}", config_path);
                        continue;
                    } else {
                        error!("Error opening config file: {}", e);
                        continue;
                    }
                }
            };
            let config = vector::topology::Config::load(file);

            match config {
                Ok(config) => {
                    topology.reload_config(config, &mut rt, require_healthy);
                }
                Err(errors) => {
                    for error in errors {
                        error!("Configuration error: {}", error);
                    }
                }
            }
        };

        if signal == SIGINT || signal == SIGTERM {
            use futures::future::Either;

            info!("Shutting down");
            topology.stop();
            metrics_trigger.cancel();

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
