#[macro_use]
extern crate tokio_trace;

use clap::{App, Arg};
use futures::{future, Future, Stream};
use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use tokio_trace::{field, Dispatch};
use tokio_trace_futures::Instrument;
use trace_metrics::MetricsSubscriber;
use vector::metrics;
use vector::topology::Topology;

fn main() {
    let app = App::new("Vector").version("0.1.0").author("timber.io")
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
                .help("The address that metrics will be served from")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("threads")
                .short("t")
                .long("threads")
                .help("Number of threads vector's core processing should use. Defaults to number of cores available")
                .takes_value(true)
        )
        .arg(Arg::with_name("verbose")
             .short("v")
             .help("Specify the verboseness of logs produced. If -q is supplied this will do nothing. Eg -v -vv")
             .multiple(true)
             .takes_value(false))
        .arg(Arg::with_name("quiet")
             .short("q")
             .help("Specify the quietness of logs produced. If -v is supplied this will override that level. Eg -q -qq")
             .conflicts_with("verbose")
             .multiple(true)
             .takes_value(false));

    let matches = app.get_matches();

    let config_path = matches.value_of("config").unwrap();
    let metrics_addr = matches.value_of("metrics-addr");

    let verboseness = matches.occurrences_of("verbose");
    let quietness = matches.occurrences_of("quiet");

    let level = if quietness > 0 {
        match quietness {
            0 => "info",
            1 => "warn",
            2 | _ => "error",
        }
    } else {
        match verboseness {
            0 => "info",
            1 => "debug",
            2 | _ => "trace",
        }
    };

    let mut levels = [
        format!("vector={}", level),
        format!("codec={}", level),
        format!("file_source={}", level),
    ]
    .join(",")
    .to_string();

    if let Ok(level) = std::env::var("LOG") {
        let additional_level = ",".to_owned() + level.as_str();
        levels.push_str(&additional_level);
    };

    let subscriber = tokio_trace_fmt::FmtSubscriber::builder()
        .with_filter(tokio_trace_fmt::filter::EnvFilter::from(levels.as_str()))
        .full()
        .finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");

    let (metrics_controller, metrics_sink) = metrics::build();
    let dispatch = if metrics_addr.is_some() {
        Dispatch::new(MetricsSubscriber::new(subscriber, metrics_sink))
    } else {
        Dispatch::new(subscriber)
    };

    tokio_trace::dispatcher::with_default(&dispatch, || {
        let threads = matches.value_of("threads").map(|string| {
            match string
                .parse::<usize>()
                .ok()
                .filter(|&t| t >= 1 && t <= 32768)
            {
                Some(t) => t,
                None => {
                    error!("threads must be a number between 1 and 32768 (inclusive)");
                    std::process::exit(1);
                }
            }
        });

        let metrics_addr = metrics_addr.map(|addr| {
            if let Ok(addr) = addr.parse() {
                addr
            } else {
                error!(
                    message = "Unable to parse metrics address.",
                    addr = field::display(&addr)
                );
                std::process::exit(1);
            }
        });

        debug!(
            message = "Loading config.",
            path = field::debug(&config_path)
        );

        let file = match std::fs::File::open(config_path) {
            Ok(f) => f,
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    error!(
                        message = "Config file not found.",
                        path = field::debug(&config_path)
                    );
                    std::process::exit(1);
                } else {
                    error!("Error opening config file: {}", e);
                    std::process::exit(1);
                }
            }
        };

        trace!(
            message = "Parsing config.",
            path = field::debug(&config_path)
        );

        let topology = vector::topology::Config::load(file).and_then(|f| {
            debug!(message = "Building config from file.");
            Topology::build(f)
        });

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

        let mut rt = {
            let mut builder = tokio::runtime::Builder::new();

            if let Some(threads) = threads {
                builder.core_threads(threads);
            }

            builder.build().expect("Unable to create async runtime")
        };

        let (metrics_trigger, metrics_tripwire) = stream_cancel::Tripwire::new();

        if let Some(metrics_addr) = metrics_addr {
            debug!("Starting metrics server");

            rt.spawn(
                metrics::serve(&metrics_addr, metrics_controller)
                    .instrument(info_span!("metrics", addr = field::display(&metrics_addr)))
                    .select(metrics_tripwire)
                    .map(|_| ())
                    .map_err(|_| ()),
            );
        }

        let require_healthy = matches.is_present("require-healthy");

        if require_healthy {
            info!("Running healthchecks and waiting to start sinks.");
            let success = rt.block_on(topology.healthchecks());

            if success.is_ok() {
                info!("All healthchecks passed.");
            } else {
                error!("Sinks unhealthy; shutting down.");
                std::process::exit(1);
            }
        } else {
            info!("Running healthchecks.");
            rt.spawn(topology.healthchecks());
        }

        info!(
            message = "Vector is starting.",
            version = built_info::PKG_VERSION,
            git_version = built_info::GIT_VERSION.unwrap_or(""),
            released = built_info::BUILT_TIME_UTC,
            arch = built_info::CFG_TARGET_ARCH
        );

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
            info!(
                message = "Reloading config.",
                path = field::debug(&config_path)
            );
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

            trace!("Parsing config");
            let config = vector::topology::Config::load(file);

            match config {
                Ok(config) => {
                    debug!("Reloading topology.");
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

            info!("Shutting down.");
            let shutdown = topology.stop();
            metrics_trigger.cancel();

            match rt.block_on(shutdown.select2(signals.into_future())) {
                Ok(Either::A(_)) => { /* Graceful shutdown finished */ }
                Ok(Either::B(_)) => {
                    info!("Shutting down immediately.");
                    // Dropping the shutdown future will immediately shut the server down
                }
                Err(_) => unreachable!(),
            }
        } else if signal == SIGQUIT {
            info!("Shutting down immediately");
            drop(topology);
        } else {
            unreachable!();
        }

        rt.shutdown_now().wait().unwrap();
    });
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
