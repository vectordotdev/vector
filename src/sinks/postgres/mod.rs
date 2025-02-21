mod wrapper;

use crate::sinks::prelude::*;
use itertools::Itertools;
use std::{collections::BTreeMap, fmt::Debug, sync::Arc};
use tokio_postgres::{types::BorrowToSql, NoTls};
use vector_lib::event::{
    metric::{MetricData, MetricSeries, MetricTime},
    EventMetadata, Metric, MetricValue,
};
use vrl::value::KeyString;
use wrapper::{JsonObjWrapper, Wrapper};

#[configurable_component(sink("postgres"))]
#[derive(Clone, Debug, Default)]
/// Write the input to a postgres tables
pub struct PostgresConfig {
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
    /// The postgress host
    pub host: String,
    /// The postgress port
    pub port: u16,
    /// The postgress table
    pub table: String,
    /// The postgress schema (default: public)
    pub schema: Option<String>,
    /// The postgress database (default: postgres)
    pub database: Option<String>,

    /// The postgres user
    pub user: Option<String>,
    /// The postgres password
    pub password: Option<String>,
}

impl_generate_config_from_default!(PostgresConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "postgres")]
impl SinkConfig for PostgresConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let PostgresConfig {
            host,
            port,
            user,
            password,
            table,
            database,
            schema,
            ..
        } = self;

        let user = user.as_deref().unwrap_or("postgres");

        let schema = schema.as_deref().unwrap_or("public");

        // dbname defaults to username if omitted so we do the same here
        let database = database.as_deref().unwrap_or(user);
        let password = password.as_deref().unwrap_or("mysecretpassword");

        let (client, connection) = tokio_postgres::connect(
            &format!("host={host} user={user} port={port} password={password} dbname={database}"),
            NoTls,
        )
        .await?;

        let client = Arc::new(client);

        let health_client = Arc::clone(&client);

        let healthcheck = Box::pin(async move {
            health_client
                .query_one("SELECT 1", Default::default())
                .await?;
            Ok(())
        });

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("connection error: {}", e);
            }
        });

        let columns: Vec<_> = client.query("SELECT column_name from INFORMATION_SCHEMA.COLUMNS WHERE table_name = $1 AND table_schema = $2", &[&table, &schema]).await?.into_iter().map(|x| x.get(0)).collect();

        let statement = client
            .prepare(&format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                columns.iter().map(|v| format!("\"{v}\"")).join(","),
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("${}", i + 1))
                    .join(",")
            ))
            .await?;

        let sink = VectorSink::from_event_streamsink(PostgresSink {
            client,
            statement,
            columns,
        });

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct PostgresSink {
    client: Arc<tokio_postgres::Client>,
    statement: tokio_postgres::Statement,
    columns: Vec<String>,
}

#[async_trait::async_trait]
impl StreamSink<Event> for PostgresSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl PostgresSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            match event {
                Event::Log(log) => self.store_log(log).await?,
                Event::Trace(trace) => self.store_trace(trace.into_parts()).await?,
                Event::Metric(metric) => self.store_metric(metric).await?,
            }
        }
        Ok(())
    }

    async fn store_log(&self, log_event: LogEvent) -> Result<(), ()> {
        let (v, mut metadata) = log_event.into_parts();

        match v {
            Value::Object(btree_map) => {
                self.store_trace((btree_map, metadata)).await?;
            }
            v if self.columns.len() == 1 => {
                match self
                    .client
                    .execute(&self.statement, &[&Wrapper(&v)])
                    .await
                    .map_err(|_| ())
                {
                    Ok(_) => metadata.update_status(EventStatus::Delivered),
                    Err(_) => metadata.update_status(EventStatus::Rejected),
                }
            }
            _ => {
                error!(
                    "Either the Value must be an object or the tables must have exactly one column"
                );
                metadata
                    .take_finalizers()
                    .update_status(EventStatus::Rejected);
                return Err(());
            }
        }

        Ok(())
    }

    async fn store_trace(
        &self,
        event: (BTreeMap<KeyString, Value>, EventMetadata),
    ) -> Result<(), ()> {
        let (v, mut metadata) = event;

        let p = self
            .columns
            .iter()
            .map(|k| v.get(k.as_str()).unwrap_or(&Value::Null))
            .map(Wrapper);

        let status = match self.client.execute_raw(&self.statement, p).await {
            Ok(_) => EventStatus::Delivered,
            Err(err) => {
                error!("{err}");
                EventStatus::Rejected
            }
        };
        metadata.take_finalizers().update_status(status);
        Ok(())
    }

    async fn store_metric(&self, metric: Metric) -> Result<(), ()> {
        let (series, data, mut metadata) = metric.into_parts();
        let MetricSeries { name, tags } = series;
        let tags = tags.map(JsonObjWrapper);
        let MetricData {
            time: MetricTime {
                timestamp,
                interval_ms,
            },
            kind,
            value,
        } = data;
        let interval_ms = interval_ms.map(|i| i.get());
        let value_wrapped = JsonObjWrapper(value);

        // Same semantics as serializing the metric into a JSON object
        // and then indexing into the resulting map, but without allocation
        let p = self
            .columns
            .iter()
            .map(|c| match (c.as_str(), &value_wrapped.0) {
                ("name", _) => name.name.borrow_to_sql(),
                ("tags", _) => tags.borrow_to_sql(),
                ("timestamp", _) => timestamp.borrow_to_sql(),
                ("interval_ms", _) => interval_ms.borrow_to_sql(),
                ("kind", _) => match kind {
                    vector_lib::event::MetricKind::Incremental => "Incremental".borrow_to_sql(),
                    vector_lib::event::MetricKind::Absolute => "Absolute".borrow_to_sql(),
                },
                ("aggregated_histogram", MetricValue::AggregatedHistogram { .. }) => {
                    value_wrapped.borrow_to_sql()
                }
                ("aggregated_summary", MetricValue::AggregatedSummary { .. }) => {
                    value_wrapped.borrow_to_sql()
                }
                ("counter", MetricValue::Counter { .. }) => value_wrapped.borrow_to_sql(),
                ("distribution", MetricValue::Distribution { .. }) => value_wrapped.borrow_to_sql(),
                ("gauge", MetricValue::Gauge { .. }) => value_wrapped.borrow_to_sql(),
                ("set", MetricValue::Set { .. }) => value_wrapped.borrow_to_sql(),
                ("sketch", MetricValue::Sketch { .. }) => value_wrapped.borrow_to_sql(),
                _ => Wrapper(&Value::Null).borrow_to_sql(),
            });

        let status = match self.client.execute_raw(&self.statement, p).await {
            Ok(_) => EventStatus::Delivered,
            Err(err) => {
                error!("{err}");
                EventStatus::Rejected
            }
        };
        metadata.take_finalizers().update_status(status);
        Ok(())
    }
}
