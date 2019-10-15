#[macro_use]
extern crate tracing;

use tracing::Dispatch;
use tracing_limit::Limit;
use tracing_subscriber::layer::SubscriberExt;

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from("trace"))
        .without_time()
        .finish()
        .with(Limit::default());

    let dispatch = Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        // This should print every 2 events
        for i in 0..40 {
            info!(message = "hello, world!", count = &i, rate_limit_secs = 5);
            trace!("this field is not rate limited!");
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    })
}
