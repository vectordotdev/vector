#[macro_use]
extern crate tracing;

use tracing::Dispatch;
use tracing_limit::Limit;
use tracing_subscriber::layer::SubscriberExt;

fn main() {
    let subscriber = tracing_fmt::FmtSubscriber::builder()
        .with_filter(tracing_fmt::filter::EnvFilter::from("trace"))
        .finish()
        .with(Limit::default());

    let dispatch = Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        // This should print every 2 events
        for i in 0..40 {
            info!(
                message = "hello, world!",
                count = &i,
                rate_limit_secs = 5 as u64
            );
            trace!("this field is not rate limited!");
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    })
}
