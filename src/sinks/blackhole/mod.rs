mod config;
mod sink;

pub use config::BlackholeConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<BlackholeConfig>("blackhole")
}

#[cfg(test)]
mod tests {

    use vector_buffers::Acker;

    use crate::{
        sinks::{
            blackhole::{config::BlackholeConfig, sink::BlackholeSink},
            VectorSink,
        },
        test_util::{
            components::run_and_assert_nonsending_sink_compliance, random_events_with_stream,
        },
    };

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
            acknowledgements: Default::default(),
        };
        let sink = BlackholeSink::new(config, Acker::passthrough());
        let sink = VectorSink::Stream(Box::new(sink));

        let (_input_lines, events) = random_events_with_stream(100, 10, None);
        run_and_assert_nonsending_sink_compliance(sink, events, &[]).await;
    }
}
