#[macro_use]
extern crate tokio_trace;

use tokio_trace::Dispatch;
use trace_limit::LimitSubscriber;

fn main() {
    let subscriber = tokio_trace_fmt::FmtSubscriber::builder().full().finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");
    let subscriber = LimitSubscriber::new(subscriber);
    let dispatch = Dispatch::new(subscriber);

    tokio_trace::dispatcher::with_default(&dispatch, || {
        // This should print every 2 events
        for i in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(333));
            info!(message = "hello, world!", count = &i, rate_limit = 3 as u64);
        }
    })
}
