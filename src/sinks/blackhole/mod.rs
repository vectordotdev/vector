mod config;
mod sink;

use crate::config::SinkDescription;

pub use config::BlackholeConfig;

inventory::submit! {
    SinkDescription::new::<BlackholeConfig>("blackhole")
}

#[cfg(test)]
mod tests {
    
    use crate::sinks::blackhole::config::BlackholeConfig;
    use crate::sinks::blackhole::sink::BlackholeSink;
    use crate::sinks::util::StreamSink;
    use crate::test_util::random_events_with_stream;
    use vector_core::buffers::Acker;

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        };
        let sink = Box::new(BlackholeSink::new(config, Acker::Null));

        let (_input_lines, events) = random_events_with_stream(100, 10, None);
        let _ = sink.run(Box::pin(events)).await.unwrap();
    }
}
