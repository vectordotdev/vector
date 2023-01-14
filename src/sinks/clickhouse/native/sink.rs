use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use super::{super::ClickhouseConfig, parse::parse_field_type, service::ClickhouseService};
use crate::sinks::clickhouse::native::service::NativeClickhouseRequest;
use crate::sinks::util::metadata::RequestMetadataBuilder;
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
use vector_common::byte_size_of::ByteSizeOf;
use vector_common::finalization::{EventFinalizers, Finalizable};
use vector_core::stream::BatcherSettings;

/// Data for a single event.
struct EventData {
    byte_size: usize,
    finalizers: EventFinalizers,
    event: Event,
}

/// Temporary struct to collect events during batching.
#[derive(Clone, Default)]
struct EventCollection {
    pub finalizers: EventFinalizers,
    pub events: Vec<Event>,
    pub events_byte_size: usize,
}

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
            //.map(|e| e.into_log())
            .map(|mut event| EventData {
                byte_size: event.size_of(),
                finalizers: event.take_finalizers(),
                event,
            })
            .batched(self.batch.into_reducer_config(
                |data: &EventData| data.event.size_of(),
                |event_collection: &mut EventCollection, item: EventData| {
                    event_collection.finalizers.merge(item.finalizers);
                    event_collection.events.push(item.event);
                    event_collection.events_byte_size += item.byte_size;
                },
            ))
            .map(|event_collection| {
                let builder = RequestMetadataBuilder::new(
                    event_collection.events.len(),
                    event_collection.events_byte_size,
                    event_collection.events_byte_size, // this is fine as it isn't being used
                );

                let bytes_len = NonZeroUsize::new(event_collection.events_byte_size)
                    .expect("payload should never be zero length");

                NativeClickhouseRequest {
                    finalizers: event_collection.finalizers,
                    metadata: builder.with_request_size(bytes_len),
                    events: event_collection.events,
                }
            })
            .into_driver(ClickhouseService::new(
                self.pool,
                self.table_schema,
                self.table,
            ))
            .run()
            .await
    }
}
