use bytes::Bytes;
use chrono::Utc;
use futures::{stream, StreamExt};
use vector_config::configurable_component;
use vector_core::ByteSizeOf;

use crate::{
    config::{log_schema, DataType, Output, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    internal_events::{InternalLogsBytesReceived, InternalLogsEventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    trace::TraceSubscription,
    SourceSender,
};

/// Configuration for the `internal_logs` source.
#[configurable_component(source)]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct InternalLogsConfig {
    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    /// The value will be the current hostname for wherever Vector is running.
    ///
    /// By default, the [global `host_key` option](https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key) is used.
    pub host_key: Option<String>,

    /// Overrides the name of the log field used to add the current process ID to each event.
    ///
    /// The value will be the current process ID for Vector itself.
    ///
    /// By default, `"pid"` is used.
    pub pid_key: Option<String>,
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

        let subscription = TraceSubscription::subscribe();

        Ok(Box::pin(run(
            host_key,
            pid_key,
            subscription,
            cx.out,
            cx.shutdown,
        )))
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
    mut subscription: TraceSubscription,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let hostname = crate::get_hostname();
    let pid = std::process::id();

    // Chain any log events that were captured during early buffering to the front,
    // and then continue with the normal stream of internal log events.
    let buffered_events = subscription.buffered_events().await;
    let mut rx = stream::iter(buffered_events.into_iter().flatten())
        .chain(subscription.into_stream())
        .take_until(shutdown);

    // Note: This loop, or anything called within it, MUST NOT generate
    // any logs that don't break the loop, as that could cause an
    // infinite loop since it receives all such logs.
    while let Some(mut log) = rx.next().await {
        let byte_size = log.size_of();
        // This event doesn't emit any log
        emit!(InternalLogsBytesReceived { byte_size });
        emit!(InternalLogsEventsReceived {
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
            emit!(StreamClosedError { error, count: 1 });
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
        // This test is fairly overloaded with different cases.
        //
        // Unfortunately, this can't be easily split out into separate test
        // cases because `consume_early_buffer` (called within the
        // `start_source` helper) panics when called more than once.
        let test_id: u8 = rand::random();
        let start = chrono::Utc::now();
        trace::init(false, false, "debug");
        trace::reset_early_buffer();

        error!(message = "Before source started without span.", %test_id);

        let span = error_span!(
            "source",
            component_kind = "source",
            component_id = "foo",
            component_type = "internal_logs",
        );
        let _enter = span.enter();

        error!(message = "Before source started.", %test_id);

        let rx = start_source().await;

        error!(message = "After source started.", %test_id);

        {
            let nested_span = error_span!(
                "nested span",
                component_kind = "bar",
                component_new_field = "baz",
                component_numerical_field = 1,
                ignored_field = "foobarbaz",
            );
            let _enter = nested_span.enter();
            error!(message = "In a nested span.", %test_id);
        }

        sleep(Duration::from_millis(1)).await;
        let mut events = collect_ready(rx).await;
        let test_id = Value::from(test_id.to_string());
        events.retain(|event| event.as_log().get("test_id") == Some(&test_id));

        let end = chrono::Utc::now();

        assert_eq!(events.len(), 4);

        assert_eq!(
            events[0].as_log()["message"],
            "Before source started without span.".into()
        );
        assert_eq!(
            events[1].as_log()["message"],
            "Before source started.".into()
        );
        assert_eq!(
            events[2].as_log()["message"],
            "After source started.".into()
        );
        assert_eq!(events[3].as_log()["message"], "In a nested span.".into());

        for (i, event) in events.iter().enumerate() {
            let log = event.as_log();
            let timestamp = *log["timestamp"]
                .as_timestamp()
                .expect("timestamp isn't a timestamp");
            assert!(timestamp >= start);
            assert!(timestamp <= end);
            assert_eq!(log["metadata.kind"], "event".into());
            assert_eq!(log["metadata.level"], "ERROR".into());
            // The first log event occurs outside our custom span
            if i == 0 {
                assert!(log.get("vector.component_id").is_none());
                assert!(log.get("vector.component_kind").is_none());
                assert!(log.get("vector.component_type").is_none());
            } else if i < 3 {
                assert_eq!(log["vector.component_id"], "foo".into());
                assert_eq!(log["vector.component_kind"], "source".into());
                assert_eq!(log["vector.component_type"], "internal_logs".into());
            } else {
                // The last event occurs in a nested span. Here, we expect
                // parent fields to be preservered (unless overwritten), new
                // fields to be added, and filtered fields to not exist.
                assert_eq!(log["vector.component_id"], "foo".into());
                assert_eq!(log["vector.component_kind"], "bar".into());
                assert_eq!(log["vector.component_type"], "internal_logs".into());
                assert_eq!(log["vector.component_new_field"], "baz".into());
                assert_eq!(log["vector.component_numerical_field"], 1.into());
                assert!(log.get("vector.ignored_field").is_none());
            }
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
        trace::stop_early_buffering();
        rx
    }
}
