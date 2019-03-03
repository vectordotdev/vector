#![allow(clippy::new_without_default, clippy::needless_pass_by_value)]

#[macro_use]
extern crate tokio_trace;

pub mod buffers;
pub mod record;
pub mod sinks;
pub mod sources;
pub mod test_util;
pub mod topology;
pub mod transforms;

pub use crate::record::Record;

use futures::{Future, Stream};
use stream_cancel::Trigger;
use tokio_signal::unix::{Signal, SIGINT, SIGQUIT, SIGTERM};
use tokio_trace_futures::{Instrument, WithSubscriber};

pub fn setup_logger() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}

pub fn run_server(
    topology: Result<
        (
            impl Future<Item = (), Error = ()> + Send + 'static,
            Trigger,
            impl Future<Item = (), Error = ()> + Send + 'static,
            Vec<String>,
        ),
        Vec<String>,
    >,
    require_healthy: bool,
) {
    info!("Building the topology...");
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

    if require_healthy {
        let success = rt.block_on(healthchecks);

        if success.is_ok() {
            info!("All healthchecks passed");
        } else {
            error!("Sinks unhealthy; shutting down");
            std::process::exit(1);
        }
    } else {
        // this works
        // let sub = tokio_trace::dispatcher::with(|subscriber| subscriber.clone());
        // rt.spawn(healthchecks.with_subscriber(sub));

        // this doesnt
        rt.spawn(healthchecks.instrument(span!("healthchecks")));
    }

    let sub = tokio_trace::dispatcher::with(|subscriber| subscriber.clone());
    rt.spawn(server.with_subscriber(sub));

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
