use crate::topology::{Config, Topology};
use futures::{Future, Stream};
use tokio_signal::unix::{Signal, SIGINT, SIGQUIT, SIGTERM};

pub fn init(config: Config, requires_healthy: bool) -> Result<(), Error> {
    let mut topology = match Topology::build(config) {
        Ok((topology, warnings)) => {
            for warning in warnings {
                error!("Configuration warning: {}", warning);
            }

            topology
        }
        Err(errors) => {
            return Err(Error::Config(errors));
        }
    };

    let mut rt = tokio::runtime::Builder::new()
        .name_prefix("vector-main-runtime-")
        .build()
        .expect("Unable to initialize runtime");

    if requires_healthy {
        let success = rt.block_on(topology.healthchecks());

        if success.is_ok() {
            info!("All healthchecks passed");
        } else {
            return Err(Error::Unhealthy);
        }
    } else {
        rt.spawn(topology.healthchecks());
    }

    let tasks = topology.start();

    for task in tasks {
        rt.spawn(task);
    }

    let signals = setup_signals();
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

    Ok(())
}

fn setup_signals() -> impl Stream<Item = i32, Error = std::io::Error> {
    let sigint = Signal::new(SIGINT).flatten_stream();
    let sigterm = Signal::new(SIGTERM).flatten_stream();
    let sigquit = Signal::new(SIGQUIT).flatten_stream();

    let signals = sigint.select(sigterm.select(sigquit));

    signals
}

pub enum Error {
    Unhealthy,
    Config(Vec<String>),
}
