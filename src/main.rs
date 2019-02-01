use clap::{App, Arg};
use futures::{Future, Stream};
use log::{error, info};
use router::topology;
use tokio_signal::unix::{Signal, SIGINT, SIGQUIT, SIGTERM};

fn main() {
    router::setup_logger();

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

    let (server, server_trigger, healthchecks) = match topology {
        Ok((server, server_trigger, healthchecks, warnings)) => {
            for warning in warnings {
                error!("Configuration warning: {}", warning);
            }

            (server, server_trigger, healthchecks)
        }
        Err(errors) => {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            return;
        }
    };

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    if matches.is_present("require-healthy") {
        let success = rt.block_on(healthchecks);

        if success.is_ok() {
            info!("All healthchecks passed");
        } else {
            error!("Sinks unhealthy; shutting down");
            std::process::exit(1);
        }
    } else {
        rt.spawn(healthchecks);
    }

    rt.spawn(server);

    let sigint = Signal::new(SIGINT).flatten_stream();
    let sigterm = Signal::new(SIGTERM).flatten_stream();
    let sigquit = Signal::new(SIGQUIT).flatten_stream();

    let signals = sigint.select(sigterm.select(sigquit));

    let (signal, signals) = rt.block_on(signals.into_future()).ok().unwrap();
    let signal = signal.unwrap();

    if signal == SIGINT || signal == SIGTERM {
        use futures::future::Either;

        info!("Shutting down");
        drop(server_trigger);

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
}
