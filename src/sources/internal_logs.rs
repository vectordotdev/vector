use bytes::Bytes;
use chrono::Utc;
use futures::{future, stream, StreamExt};
use serde::{Deserialize, Serialize};
use vector_core::ByteSizeOf;

use crate::{
    config::{log_schema, DataType, Output, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    internal_events::{InternalLogsBytesReceived, InternalLogsEventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    trace, SourceSender,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InternalLogsConfig {
    host_key: Option<String>,
    pid_key: Option<String>,
}

inventory::submit! {
    SourceDescription::new::<InternalLogsConfig>("internal_logs")
}

impl_generate_config_from_default!(InternalLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_logs")]
impl SourceConfig for InternalLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let host_key = self
            .host_key
            .as_deref()
            .unwrap_or_else(|| log_schema().host_key())
            .to_owned();
        let pid_key = self.pid_key.as_deref().unwrap_or("pid").to_owned();

        Ok(Box::pin(run(host_key, pid_key, cx.out, cx.shutdown)))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "internal_logs"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn run(
    host_key: String,
    pid_key: String,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let hostname = crate::get_hostname();
    let pid = std::process::id();

    let subscription = trace::subscribe();

    // chain the logs emitted before the source started first
    let mut rx = stream::iter(subscription.buffer)
        .map(Ok)
        .chain(tokio_stream::wrappers::BroadcastStream::new(
            subscription.receiver,
        ))
        .filter_map(|log| future::ready(log.ok()))
        .take_until(shutdown);

    // Note: This loop, or anything called within it, MUST NOT generate
    // any logs that don't break the loop, as that could cause an
    // infinite loop since it receives all such logs.
    while let Some(mut log) = rx.next().await {
        let byte_size = log.size_of();
        // This event doesn't emit any log
        emit!(&InternalLogsBytesReceived { byte_size });
        emit!(&InternalLogsEventsReceived {
            count: 1,
            byte_size,
        });
        if let Ok(hostname) = &hostname {
            log.insert(host_key.as_str(), hostname.to_owned());
        }
        log.insert(pid_key.as_str(), pid);
        log.try_insert(log_schema().source_type_key(), Bytes::from("internal_logs"));
        log.try_insert(log_schema().timestamp_key(), Utc::now());
        if let Err(error) = out.send_event(Event::from(log)).await {
            // this wont trigger any infinite loop considering it stops the component
            emit!(&StreamClosedError { error, count: 1 });
            return Err(());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use futures::Stream;
    use tokio::time::{sleep, Duration};
    use vector_core::event::Value;

    use super::*;
    use crate::{event::Event, test_util::collect_ready, trace};

    #[test]
    fn generates_config() {
        crate::test_util::test_generate_config::<InternalLogsConfig>();
    }

    #[tokio::test]
    async fn receives_logs() {
        let test_id: u8 = rand::random();
        let start = chrono::Utc::now();
        trace::init(false, false, "debug");
        trace::reset_early_buffer();
        error!(message = "Before source started.", %test_id);

        let rx = start_source().await;

        error!(message = "After source started.", %test_id);

        sleep(Duration::from_millis(1)).await;
        let mut events = collect_ready(rx).await;
        let test_id = Value::from(test_id.to_string());
        events.retain(|event| event.as_log().get("test_id") == Some(&test_id));

        let end = chrono::Utc::now();

        assert_eq!(events.len(), 2);

        assert_eq!(
            events[0].as_log()["message"],
            "Before source started.".into()
        );
        assert_eq!(
            events[1].as_log()["message"],
            "After source started.".into()
        );

        for event in events {
            let log = event.as_log();
            let timestamp = *log["timestamp"]
                .as_timestamp()
                .expect("timestamp isn't a timestamp");
            assert!(timestamp >= start);
            assert!(timestamp <= end);
            assert_eq!(log["metadata.kind"], "event".into());
            assert_eq!(log["metadata.level"], "ERROR".into());
        }
    }

    async fn start_source() -> impl Stream<Item = Event> {
        let (tx, rx) = SourceSender::new_test();

        let source = InternalLogsConfig::default()
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(source);
        sleep(Duration::from_millis(1)).await;
        trace::stop_buffering();
        rx
    }
}
