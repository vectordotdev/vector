use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use futures::Stream;
use futures_util::stream;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use tokio::time::sleep;
use vector_config::schema::generate_string_schema;
use vector_config::{Configurable, GenerateError, Metadata, ToValue};
use vector_config_common::schema::{SchemaGenerator, SchemaObject};

/// Newtype around `cron::Schedule` that enables a `Configurable` implementation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OdbcSchedule {
    inner: Schedule,
}

impl ToValue for OdbcSchedule {
    fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(&self.inner)
            .expect("Could not convert schedule(cron expression) to JSON")
    }
}

impl Configurable for OdbcSchedule {
    fn referenceable_name() -> Option<&'static str> {
        Some("cron::Schedule")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("Cron expression in seconds.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl From<&str> for OdbcSchedule {
    fn from(s: &str) -> Self {
        let schedule = Schedule::from_str(s).expect("Invalid cron expression");
        Self { inner: schedule }
    }
}

impl Debug for OdbcSchedule {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.to_string())
    }
}

impl OdbcSchedule {
    /// Creates a stream that asynchronously waits for each scheduled cron time.
    pub(crate) fn stream(self, tz: Tz) -> impl Stream<Item = DateTime<Tz>> {
        let schedule = self.inner.clone();
        stream::unfold(schedule, move |schedule| async move {
            let now = Utc::now().with_timezone(&tz);
            let mut upcoming = schedule.upcoming(tz);
            let next = upcoming.next()?;
            let delay = (next - now).abs();

            sleep(delay.to_std().unwrap_or_default()).await;
            Some((next, schedule))
        })
    }
}
