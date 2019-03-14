#[macro_use]
extern crate tokio_trace;
extern crate trace_metrics;
extern crate hotmic;

use hotmic::Receiver;
use std::thread;

fn shave(yak: usize) -> bool {
    span!("shave", yak = yak).enter(|| {
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

    thread::spawn(move || {
        receiver.run();
    });
    
    let subscriber = trace_metrics::MetricsSubscriber::new(sink);

    tokio_trace::subscriber::with_default(subscriber, || {
        let number_of_yaks = 3;
        let mut number_shaved = 0;
        debug!("preparing to shave {} yaks", number_of_yaks);

        span!("shaving_yaks", yaks_to_shave = number_of_yaks).enter(|| {
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
}
