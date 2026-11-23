use std::{
    fmt,
    sync::{Arc, Mutex},
};

use tokio::sync::mpsc;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
};

use crate::{
    config::ComponentKey,
    signal::ShutdownError,
    topology::{
        handle_errors,
        task::{TaskError, TaskOutput},
    },
};

#[derive(Default)]
struct LogFields {
    message: Option<String>,
    panic: Option<String>,
}

impl Visit for LogFields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "panic" {
            self.panic = Some(value.to_owned());
        }
    }
}

struct Capture(Arc<Mutex<Vec<LogFields>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = LogFields::default();
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn panic_payload_is_logged_as_a_field() {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(Arc::clone(&logs)));
    let _guard = tracing::subscriber::set_default(subscriber);
    let (abort_tx, mut abort_rx) = mpsc::unbounded_channel();

    let task = async {
        std::panic::panic_any("source panic value");
        #[allow(unreachable_code)]
        Ok(TaskOutput::Source)
    };
    let result = handle_errors(task, abort_tx, |error| ShutdownError::SourceAborted {
        key: ComponentKey::from("source"),
        error,
    })
    .await;

    assert!(matches!(result, Err(TaskError::Panicked { .. })));
    let shutdown_error = abort_rx.recv().await.unwrap();
    assert!(!shutdown_error.to_string().contains("source panic value"));

    let logs = logs.lock().unwrap();
    let log = logs.iter().find(|log| log.panic.is_some()).unwrap();
    assert_eq!(log.panic.as_deref(), Some("source panic value"));
    assert!(!log.message.as_ref().unwrap().contains("source panic value"));
}

#[tokio::test(flavor = "current_thread")]
async fn joined_task_panic_retains_payload() {
    let error = tokio::spawn(async { std::panic::panic_any(String::from("child panic value")) })
        .await
        .unwrap_err();
    let error = TaskError::from(error);
    assert!(matches!(
        error,
        TaskError::Panicked { panic } if panic == "child panic value"
    ));
}
