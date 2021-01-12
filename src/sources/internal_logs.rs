use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    shutdown::ShutdownSignal,
    trace, Pipeline,
};
use futures::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::RecvError;

#[serde(deny_unknown_fields)]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InternalLogsConfig {}

inventory::submit! {
    SourceDescription::new::<InternalLogsConfig>("internal_logs")
}

impl_generate_config_from_default!(InternalLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_logs")]
impl SourceConfig for InternalLogsConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        Ok(Box::pin(run(out, shutdown)))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "internal_logs"
    }
}

async fn run(out: Pipeline, shutdown: ShutdownSignal) -> Result<(), ()> {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending log.", %error));
    let subscription = trace::subscribe();
    let mut subscriber = subscription.receiver.take_until(shutdown);

    out.send_all(&mut stream::iter(subscription.buffer).map(Ok))
        .await?;

    // Note: This loop, or anything called within it, MUST NOT generate
    // any logs that don't break the loop, as that could cause an
    // infinite loop since it receives all such logs.

    while let Some(receive) = subscriber.next().await {
        match receive {
            Ok(event) => out.send(event).await?,
            Err(RecvError::Lagged(_)) => (),
            Err(RecvError::Closed) => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::GlobalOptions, test_util::collect_ready, trace, Event};
    use tokio::{
        sync::mpsc::Receiver,
        time::{delay_for, Duration},
    };

    #[test]
    fn generates_config() {
        crate::test_util::test_generate_config::<InternalLogsConfig>();
    }

    const ERROR_TEXT: &str = "This is not an error.";

    #[tokio::test]
    async fn receives_logs() {
        let start = chrono::Utc::now();
        trace::init(false, false, "debug");

        let rx = start_source().await;
        error!(message = ERROR_TEXT);
        let logs = collect_output(rx).await;

        check_events(logs, start);
    }

    #[tokio::test]
    async fn receives_early_logs() {
        let start = chrono::Utc::now();
        trace::init(false, false, "debug");
        trace::reset_early_buffer();
        error!(message = ERROR_TEXT);

        let rx = start_source().await;
        let logs = collect_output(rx).await;

        check_events(logs, start);
    }

    async fn start_source() -> Receiver<Event> {
        let (tx, rx) = Pipeline::new_test();

        let source = InternalLogsConfig {}
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap();
        tokio::spawn(source);
        delay_for(Duration::from_millis(1)).await;
        trace::stop_buffering();
        rx
    }

    async fn collect_output(rx: Receiver<Event>) -> Vec<Event> {
        delay_for(Duration::from_millis(1)).await;
        collect_ready(rx).await
    }

    fn check_events(events: Vec<Event>, start: chrono::DateTime<chrono::Utc>) {
        let end = chrono::Utc::now();

        assert_eq!(events.len(), 1);

        let log = events[0].as_log();
        assert_eq!(log["message"], ERROR_TEXT.into());
        let timestamp = *log["timestamp"]
            .as_timestamp()
            .expect("timestamp isn't a timestamp");
        assert!(timestamp >= start);
        assert!(timestamp <= end);
        assert_eq!(log["metadata.kind"], "event".into());
        assert_eq!(log["metadata.level"], "ERROR".into());
    }
}
