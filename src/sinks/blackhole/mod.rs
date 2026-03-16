mod config;
mod sink;

pub use config::BlackholeConfig;

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::{
        sinks::{
            VectorSink,
            blackhole::{config::BlackholeConfig, sink::BlackholeSink},
        },
        test_util::{
            components::{SINK_TAGS, run_and_assert_sink_compliance},
            random_events_with_stream,
        },
    };

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig {
            print_interval_secs: Duration::from_secs(10),
            rate: None,
            acknowledgements: Default::default(),
        };
        let sink = BlackholeSink::new(config);
        let sink = VectorSink::Stream(Box::new(sink));

        let (_input_lines, events) = random_events_with_stream(100, 10, None);
        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    }
}
