use std::{collections::BTreeMap, sync::Arc};

use crate::sinks::prelude::*;
use bytes::{Buf, BufMut};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use tokio_postgres::{
    types::{to_sql_checked, IsNull, ToSql},
    NoTls,
};
use vector_lib::event::EventMetadata;
use vrl::value::KeyString;

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

#[derive(Debug)]
struct Wrapper<'a>(&'a Value);

impl ToSql for Wrapper<'_> {
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
            Value::Object(btree_map) => {
                serde_json::to_writer(out.writer(), btree_map)?;
                Ok(IsNull::No)
            }
            Value::Array(values) => {
                // Taken from postgres-types/lib.rs `impl<T: ToSql> ToSql for &[T]`
                //
                // There is no function that serializes an iterator, only a method on slices,
                // but we should not have to allocate a new `Vec<Wrapper<&Value>>` just to
                // serialize the `Vec<Value>` we already have

                let member_type = match *ty.kind() {
                    tokio_postgres::types::Kind::Array(ref member) => member,
                    _ => {
                        return Err(Box::new(
                            tokio_postgres::types::WrongType::new::<Vec<Value>>(ty.clone()),
                        ))
                    }
                };

                // Arrays are normally one indexed by default but oidvector and int2vector *require* zero indexing
                let lower_bound = match *ty {
                    tokio_postgres::types::Type::OID_VECTOR
                    | tokio_postgres::types::Type::INT2_VECTOR => 0,
                    _ => 1,
                };

                let dimension = postgres_protocol::types::ArrayDimension {
                    len: values.len().try_into()?,
                    lower_bound,
                };

                postgres_protocol::types::array_to_sql(
                    Some(dimension),
                    member_type.oid(),
                    values.iter().map(Wrapper),
                    |e, w| match e.to_sql(member_type, w)? {
                        IsNull::No => Ok(postgres_protocol::IsNull::No),
                        IsNull::Yes => Ok(postgres_protocol::IsNull::Yes),
                    },
                    out,
                )?;
                Ok(IsNull::No)
            }
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
            || match *ty.kind() {
                tokio_postgres::types::Kind::Array(ref member) => Self::accepts(member),
                _ => false,
            }
    }

    to_sql_checked!();
}

impl PostgresSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(mut event) = input.next().await {
            match event {
                Event::Log(log_event) => {
                    let (v, mut metadata) = log_event.into_parts();

                    match v {
                        Value::Object(btree_map) => {
                            self.store_trace((btree_map, metadata)).await?;
                        }
                        v if self.columns.len() == 1 => {
                            self.client
                                .execute(&self.statement, &[&Wrapper(&v)])
                                .await
                                .map_err(|_| ())?;
                        }
                        _ => {
                            error!("Either the Value must be an object or the tables must have exactly one column");
                            metadata
                                .take_finalizers()
                                .update_status(EventStatus::Rejected);
                            return Err(());
                        }
                    }
                }
                Event::Trace(trace) => self.store_trace(trace.into_parts()).await?,
                _ => {
                    error!("Only logs are implemented so far");
                    event.take_finalizers().update_status(EventStatus::Rejected);
                }
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
}
