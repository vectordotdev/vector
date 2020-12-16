use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{SinkExt, StreamExt};
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
    let mut subscriber = crate::trace::subscribe()
        .ok_or_else(|| error!("Tracing is not initialized."))?
        .take_until(shutdown);
    let mut out = out.sink_map_err(|error| error!(message = "Error sending log.", %error));

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
    use crate::{config::GlobalOptions, test_util::collect_ready};
    use tokio::time::{delay_for, Duration};

    #[test]
    fn generates_config() {
        crate::test_util::test_generate_config::<InternalLogsConfig>();
    }

    #[tokio::test]
    async fn receives_logs() {
        const ERROR_TEXT: &str = "This is not an error.";

        let start = chrono::Utc::now();
        crate::trace::init(false, false, "debug");
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

        error!(message = ERROR_TEXT);

        delay_for(Duration::from_millis(1)).await;
        let logs = collect_ready(rx).await;

        assert_eq!(logs.len(), 1);

        let log = logs[0].as_log();
        assert_eq!(log["message"], ERROR_TEXT.into());
        assert!(
            log["timestamp"]
                .as_timestamp()
                .expect("timestamp isn't a timestamp")
                > &start
        );
        assert_eq!(log["metadata.kind"], "event".into());
        assert_eq!(log["metadata.level"], "ERROR".into());
    }
}
