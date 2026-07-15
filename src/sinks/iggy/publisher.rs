//! Iggy producer — vendored from `obstack_queue::producer::IggyPublisher`.
//!
//! Connects to Iggy, validates that the target stream/topic exists with the
//! configured partition (= shard) count, captures the broker "generation"
//! (numeric ids + creation timestamps) that every envelope is stamped with,
//! and publishes shard-split, size-chunked, fsync-durable batches.
#![allow(dead_code)]

use std::sync::Arc;

use bytes::Bytes;
use futures::future::join_all;
use iggy::prelude::*;

use super::proto::{
    QueueGeneration, WriteBatch, encode_chunks, stable_message_id,
};

/// Producer holding a small pool of Iggy connections ("lanes"), one used per
/// shard so independent partitions append and fsync concurrently.
pub struct IggyPublisher {
    clients: Vec<Arc<IggyClient>>,
    stream: Identifier,
    topic: Identifier,
    generation: QueueGeneration,
    shards: u32,
    max_message_bytes: usize,
}

impl IggyPublisher {
    pub async fn connect(
        connection_string: &str,
        stream: &str,
        topic: &str,
        shards: u32,
        max_message_bytes: usize,
        lanes: usize,
    ) -> crate::Result<Self> {
        if shards == 0 {
            return Err("shard count must be positive".into());
        }
        if max_message_bytes == 0 || max_message_bytes > 64_000_000 {
            return Err("max_message_bytes must be between 1 and Iggy's 64000000-byte limit".into());
        }
        let lanes = lanes.clamp(1, shards as usize);

        let first = IggyClient::from_connection_string(connection_string)?;
        first.connect().await?;
        let stream_name: Identifier = stream.try_into()?;
        let topic_name: Identifier = topic.try_into()?;
        let stream_details = first
            .get_stream(&stream_name)
            .await?
            .ok_or_else(|| format!("Iggy stream {stream} is missing"))?;
        let topic_details = first
            .get_topic(&stream_name, &topic_name)
            .await?
            .ok_or_else(|| format!("Iggy topic {stream}/{topic} is missing"))?;
        if topic_details.partitions_count != shards {
            return Err(format!(
                "Iggy topic has {} partitions, sink is configured for {shards} shards",
                topic_details.partitions_count
            )
            .into());
        }
        let generation = QueueGeneration {
            stream_id: stream_details.id,
            stream_created_at_micros: stream_details.created_at.as_micros(),
            topic_id: topic_details.id,
            topic_created_at_micros: topic_details.created_at.as_micros(),
        };
        // Pin subsequent commands to this incarnation's numeric ids.
        let stream_id = Identifier::numeric(stream_details.id)?;
        let topic_id = Identifier::numeric(topic_details.id)?;

        let mut clients = Vec::with_capacity(lanes);
        clients.push(Arc::new(first));
        let connections = (1..lanes).map(|_| async move {
            let client = IggyClient::from_connection_string(connection_string)?;
            client.connect().await?;
            Ok::<_, crate::Error>(Arc::new(client))
        });
        for result in join_all(connections).await {
            clients.push(result?);
        }
        Ok(Self {
            clients,
            stream: stream_id,
            topic: topic_id,
            generation,
            shards,
            max_message_bytes,
        })
    }

    pub fn shards(&self) -> u32 {
        self.shards
    }

    /// Split a tenant batch by shard, encode v3 envelopes, and durably
    /// publish each partition (append + explicit fsync). Retry-safe: the
    /// store upserts every row by natural key on redelivery.
    pub async fn publish(&self, batch: WriteBatch) -> crate::Result<()> {
        let parts = batch
            .split_by_shard(self.shards)
            .map_err(|e| e.to_string())?;
        let mut prepared = Vec::with_capacity(parts.len());
        for (shard, batch) in parts {
            let payloads = encode_chunks(
                self.generation,
                shard,
                self.shards,
                batch,
                self.max_message_bytes,
            )
            .map_err(|e| e.to_string())?;
            let mut messages = Vec::with_capacity(payloads.len());
            for (sequence, payload) in payloads.into_iter().enumerate() {
                let id = stable_message_id(shard, sequence as u64, &payload);
                messages.push(
                    IggyMessage::builder()
                        .id(id)
                        .payload(Bytes::from(payload))
                        .build()?,
                );
            }
            prepared.push((shard, messages));
        }

        let sends = prepared
            .into_iter()
            .map(|(partition, mut messages)| async move {
                let partitioning = Partitioning::partition_id(partition);
                let client = &self.clients[partition as usize % self.clients.len()];
                client
                    .send_messages(&self.stream, &self.topic, &partitioning, &mut messages)
                    .await?;
                client
                    .flush_unsaved_buffer(&self.stream, &self.topic, partition, true)
                    .await?;
                Result::<u32, IggyError>::Ok(partition)
            });
        let mut first_error = None;
        for result in join_all(sends).await {
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn shutdown(&self) -> crate::Result<()> {
        for result in join_all(self.clients.iter().map(|client| client.shutdown())).await {
            result?;
        }
        Ok(())
    }
}
