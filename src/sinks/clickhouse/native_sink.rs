use std::collections::BTreeMap;

use futures::{stream::BoxStream, StreamExt};
use clickhouse_rs::{Pool, types::SqlType,};
use vector_core::stream::BatcherSettings;
use async_trait::async_trait;


use super::{ClickhouseConfig, native_service::ClickhouseService, parse::parse_sql_type};
use crate::{
    config::{SinkContext, SinkHealthcheckOptions},
    event::Event,
    sinks::{
        util::{StreamSink, SinkBuilderExt},
        Healthcheck, VectorSink,
    },
};

pub(super) async fn build_native_sink(cfg: &ClickhouseConfig, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
    let table_schema = gen_table_schema(&cfg.table_def)?;
    let batch = cfg.batch.into_batcher_settings()?;
    let pool = Pool::new(cfg.endpoint.to_string());
    let sink = NativeClickhouseSink::new(pool.clone(), batch, cfg.table.clone(), table_schema);
    let health_check = healthcheck(pool, cx.healthcheck.clone());
    Ok((
        VectorSink::from_event_streamsink(sink),
        Box::pin(health_check)
    ))
}

fn gen_table_schema(table: &BTreeMap<String, String>) -> crate::Result<Vec<(String, SqlType)>> {
    let mut table_schema = Vec::with_capacity(table.len());
    for (k,v) in table {
        match parse_sql_type(v.as_str()) {
            Ok((_, t)) => {
                table_schema.push((k.to_owned(), t));
            },
            Err(e) => {
                let ne: crate::Error = std::convert::From::from(e.to_string());
                Err(ne)?;
            }
        }
    }
    Ok(table_schema)
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
    fn new(pool: Pool, batch: BatcherSettings, table: String, table_schema: Vec<(String, SqlType)> ) -> Self {
        Self {
            pool,
            batch,
            table,
            table_schema,
        }
    }
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input.map(|e| e.into_log())
            .batched(self.batch.into_byte_size_config())
            .into_driver(ClickhouseService::new(self.pool, self.table_schema, self.table))
            .run().await
    }

}

#[async_trait]
impl StreamSink<Event> for NativeClickhouseSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}