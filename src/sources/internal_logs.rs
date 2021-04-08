use crate::{
    config::{DataType, SourceConfig, SourceContext, SourceDescription},
    shutdown::ShutdownSignal,
    trace, Pipeline,
};
use futures::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InternalLogsConfig {}

inventory::submit! {
    SourceDescription::new::<InternalLogsConfig>("internal_logs")
}

impl_generate_config_from_default!(InternalLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_logs")]
impl SourceConfig for InternalLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        Ok(Box::pin(run(cx.out, cx.shutdown)))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "internal_logs"
    }
}

async fn run(out: Pipeline, mut shutdown: ShutdownSignal) -> Result<(), ()> {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending log.", %error));
    let subscription = trace::subscribe();
    let mut rx = subscription.receiver;

    out.send_all(&mut stream::iter(subscription.buffer).map(Ok))
        .await?;

    // Note: This loop, or anything called within it, MUST NOT generate
    // any logs that don't break the loop, as that could cause an
    // infinite loop since it receives all such logs.
    loop {
        tokio::select! {
            receive = rx.recv() => {
                match receive {
                    Ok(event) => out.send(event).await?,
                    Err(RecvError::Lagged(_)) => (),
                    Err(RecvError::Closed) => break,
                }
            }
            _ = &mut shutdown => break,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_util::collect_ready, trace, Event};
    use futures::channel::mpsc;
    use tokio::time::{sleep, Duration};

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

    async fn start_source() -> mpsc::Receiver<Event> {
        let (tx, rx) = Pipeline::new_test();

        let source = InternalLogsConfig {}
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(source);
        sleep(Duration::from_millis(1)).await;
        trace::stop_buffering();
        rx
    }

    async fn collect_output(rx: mpsc::Receiver<Event>) -> Vec<Event> {
        sleep(Duration::from_millis(1)).await;
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
