use tracing::{info, info_span, trace, Dispatch};
use tracing_limit::RateLimitedLayer;
use tracing_subscriber::layer::SubscriberExt;

fn main() {
    let subscriber = tracing_subscriber::registry::Registry::default()
        .with(RateLimitedLayer::new(
            tracing_subscriber::fmt::Layer::default().without_time(),
        ))
        .with(tracing_subscriber::filter::EnvFilter::from("trace"));

    let dispatch = Dispatch::new(subscriber);

    tracing::dispatcher::with_default(&dispatch, || {
        for i in 0..40usize {
            trace!("This field is not rate limited!");
            for key in &["foo", "bar"] {
                for line_number in &[1, 2] {
                    let span = info_span!(
                        "sink",
                        component_kind = "sink",
                        component_id = &key,
                        component_type = "fake",
                        vrl_line_number = &line_number,
                    );
                    let _enter = span.enter();
                    info!(
                        message =
                            "This message is rate limited by its component and vrl_line_number",
                        count = &i,
                        internal_log_rate_limit = true,
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    })
}
