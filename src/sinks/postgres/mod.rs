use crate::sinks::prelude::*;
use bytes::Buf;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use tokio_postgres::{
    types::{to_sql_checked, IsNull, ToSql},
    NoTls,
};

#[configurable_component(sink("postgres"))]
#[derive(Clone, Debug)]
/// Write the input to a postgres tables
pub struct BasicConfig {
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

impl GenerateConfig for BasicConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "postgres")]
impl SinkConfig for BasicConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let healthcheck = Box::pin(async move { Ok(()) });

        let BasicConfig {
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

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("connection error: {}", e);
            }
        });

        let columns = client.query("SELECT column_name from INFORMATION_SCHEMA.COLUMNS WHERE table_name = $1 AND table_schema = $2", &[&table, &schema]).await?.into_iter().map(|x| x.get(0)).collect();

        let sink = VectorSink::from_event_streamsink(PostgresSink {
            client,
            table: table.to_owned(),
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
    client: tokio_postgres::Client,
    table: String,
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

#[derive(Debug)]
struct Wrapper<'a>(&'a Value);

impl<'a> ToSql for Wrapper<'a> {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        match self.0 {
            Value::Bytes(bytes) => bytes.chunk().to_sql(ty, out),
            Value::Regex(value_regex) => value_regex.as_str().to_sql(ty, out),
            Value::Integer(i) => i.to_sql(ty, out),
            Value::Float(not_nan) => not_nan.to_sql(ty, out),
            Value::Boolean(b) => b.to_sql(ty, out),
            Value::Timestamp(date_time) => date_time.to_sql(ty, out),
            Value::Object(btree_map) => serde_json::to_value(btree_map).unwrap().to_sql(ty, out),
            Value::Array(values) => values.iter().map(Wrapper).collect_vec().to_sql(ty, out),
            Value::Null => Ok(IsNull::Yes),
        }
    }

    fn accepts(ty: &tokio_postgres::types::Type) -> bool
    where
        Self: Sized,
    {
        <&[u8]>::accepts(ty)
            || <&str>::accepts(ty)
            || i64::accepts(ty)
            || f64::accepts(ty)
            || bool::accepts(ty)
            || DateTime::<Utc>::accepts(ty)
            || serde_json::Value::accepts(ty)
            || Option::<u32>::accepts(ty)
    }

    to_sql_checked!();
}

impl PostgresSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let Self { table, .. } = self.as_ref();

        let statement = self
            .client
            .prepare(&format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                self.columns.iter().map(|v| format!("\"{v}\"")).join(","),
                self.columns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("${}", i + 1))
                    .join(",")
            ))
            .await
            .unwrap();

        while let Some(event) = input.next().await {
            match event {
                Event::Log(log_event) => {
                    let (v, mut metadata) = log_event.into_parts();

                    let v = v.into_object().unwrap();

                    let p = self
                        .columns
                        .iter()
                        .map(|k| v.get(k.as_str()).unwrap_or(&Value::Null))
                        .map(Wrapper);

                    let status = match self.client.execute_raw(&statement, p).await {
                        Ok(_) => EventStatus::Delivered,
                        Err(err) => {
                            error!("{err}");
                            EventStatus::Rejected
                        }
                    };
                    metadata.take_finalizers().update_status(status)
                }
                _ => todo!("Only logs are implemented so far"),
            }
        }

        Ok(())
    }
}
