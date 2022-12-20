use std::collections::BTreeMap;

use super::{super::ClickhouseConfig, service::ClickhouseService, parse::parse_field_type};
use crate::{
    config::{SinkContext, SinkHealthcheckOptions},
    event::Event,
    sinks::{
        util::{SinkBuilderExt, StreamSink},
        Healthcheck, VectorSink,
    },
};
use async_trait::async_trait;
use clickhouse_rs::{types::SqlType, Pool};
use futures::{stream::BoxStream, StreamExt};
use vector_core::stream::BatcherSettings;

pub async fn build_native_sink(
    cfg: &ClickhouseConfig,
    cx: SinkContext,
) -> crate::Result<(VectorSink, Healthcheck)> {
    let table_schema = gen_table_schema(&cfg.sql_table_col_def)?;
    let batch = cfg.batch.into_batcher_settings()?;
    let pool = Pool::new(cfg.endpoint.to_string());
    let sink = NativeClickhouseSink::new(pool.clone(), batch, cfg.table.clone(), table_schema);
    let health_check = healthcheck(pool, cx.healthcheck);
    Ok((
        VectorSink::from_event_streamsink(sink),
        Box::pin(health_check),
    ))
}

fn gen_table_schema(table: &BTreeMap<String, String>) -> crate::Result<Vec<(String, SqlType)>> {
    table
        .iter()
        .map(|(k, v)| {
            parse_field_type(v.as_str())
                .map(|(_, t)| (k.to_owned(), t))
                .map_err(|e| e.to_string().into())
        })
        .collect()
}

async fn healthcheck(pool: Pool, opts: SinkHealthcheckOptions) -> crate::Result<()> {
    if !opts.enabled {
        return Ok(());
    }
    let mut client = pool.get_handle().await?;
    client.ping().await.map_err(|e| e.into())
}

struct NativeClickhouseSink {
    pool: Pool,
    batch: BatcherSettings,
    table: String,
    table_schema: Vec<(String, SqlType)>,
}

impl NativeClickhouseSink {
    fn new(
        pool: Pool,
        batch: BatcherSettings,
        table: String,
        table_schema: Vec<(String, SqlType)>,
    ) -> Self {
        Self {
            pool,
            batch,
            table,
            table_schema,
        }
    }
}

#[async_trait]
impl StreamSink<Event> for NativeClickhouseSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|e| e.into_log())
            .batched(self.batch.into_byte_size_config())
            .into_driver(ClickhouseService::new(
                self.pool,
                self.table_schema,
                self.table,
            ))
            .run()
            .await
    }
}
