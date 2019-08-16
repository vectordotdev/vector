#[macro_use]
extern crate tracing;

use hotmic::Receiver;
use std::thread;

fn shave(yak: usize) -> bool {
    trace_span!("shave", yak = yak).in_scope(|| {
        debug!(
            message = "hello! I'm gonna shave a yak.",
            excitement = "yay!"
        );
        if yak == 3 {
            warn!(target: "yak_events", "could not locate yak!");
            false
        } else {
            trace!(target: "yak_events", "yak shaved successfully");
            true
        }
    })
}

fn main() {
    let mut receiver = Receiver::builder().build();
    let sink = receiver.get_sink();
    let controller = receiver.get_controller();

    thread::spawn(move || {
        receiver.run();
    });

    let subscriber = tracing_fmt::FmtSubscriber::builder().finish();
    tracing_env_logger::try_init().expect("init log adapter");
    let subscriber = tracing_metrics::MetricsSubscriber::new(subscriber, sink);

    tracing::subscriber::with_default(subscriber, || {
        let number_of_yaks = 3;
        let mut number_shaved = 0;
        debug!("preparing to shave {} yaks", number_of_yaks);

        trace_span!("shaving_yaks", yaks_to_shave = number_of_yaks).in_scope(|| {
            info!("shaving yaks");

            for yak in 1..=number_of_yaks {
                let shaved = shave(yak);
                trace!(target: "yak_events", yak = yak, shaved = shaved);

                if !shaved {
                    error!(message = "failed to shave yak!", yak = yak);
                } else {
                    number_shaved += 1;
                }

                trace!(target: "yak_events", yaks_shaved = number_shaved);
            }
        });

        debug!(
            message = "yak shaving completed.",
            all_yaks_shaved = number_shaved == number_of_yaks,
        );
    });

    let _snapshot = controller.get_snapshot().unwrap();
    // let raw_snap = serde_json::to_string_pretty(&snapshot).unwrap();

    // println!("Metrics snapshot: {}", raw_snap);
}
