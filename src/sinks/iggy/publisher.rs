//! Dynamic producer-scoped Iggy publisher for Obstack.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::future::join_all;
use iggy::prelude::*;
use tokio::sync::{Mutex, OnceCell};

use super::proto::{
    PRODUCER_PARTITIONS, ProducerIdentity, ProducerRegistration, QueueGeneration, WriteBatch,
    decode_registration, encode_chunks, encode_registration, registration_message_id,
    stable_message_id,
};

struct TopicPublisher {
    topic: Identifier,
    generation: QueueGeneration,
}

/// One bounded connection pool shared by every producer topic. Topic metadata
/// is cached independently, avoiding one socket per high-cardinality producer.
pub struct IggyPublisher {
    clients: Vec<Arc<IggyClient>>,
    stream: Identifier,
    stream_id: u32,
    stream_created_at_micros: u64,
    replication_factor: u8,
    max_message_bytes: usize,
    max_active_topics: usize,
    bootstrap_timeout: Duration,
    topics: Mutex<HashMap<ProducerIdentity, Arc<OnceCell<Arc<TopicPublisher>>>>>,
}

impl IggyPublisher {
    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
        connection_string: &str,
        stream: &str,
        max_message_bytes: usize,
        lanes: usize,
        replication_factor: u8,
        max_active_topics: usize,
        bootstrap_timeout: Duration,
    ) -> crate::Result<Self> {
        if max_message_bytes == 0 || max_message_bytes > 64_000_000 {
            return Err(
                "max_message_bytes must be between 1 and Iggy's 64000000-byte limit".into(),
            );
        }
        if lanes == 0 || lanes > 64 || replication_factor == 0 || max_active_topics == 0 {
            return Err("lanes, replication_factor, and max_active_topics must be positive".into());
        }
        let first = IggyClient::from_connection_string(connection_string)?;
        first.connect().await?;
        let stream_name: Identifier = stream.try_into()?;
        let stream_details = match first.get_stream(&stream_name).await? {
            Some(details) => details,
            None => match first.create_stream(stream).await {
                Ok(details) => details,
                Err(_) => first
                    .get_stream(&stream_name)
                    .await?
                    .ok_or_else(|| format!("Iggy stream {stream} cannot be created or read"))?,
            },
        };
        let stream_id = Identifier::numeric(stream_details.id)?;
        let mut clients = vec![Arc::new(first)];
        let connections = (1..lanes).map(|_| async move {
            let client = IggyClient::from_connection_string(connection_string)?;
            client.connect().await?;
            Ok::<_, crate::Error>(Arc::new(client))
        });
        for client in join_all(connections).await {
            clients.push(client?);
        }
        Ok(Self {
            clients,
            stream: stream_id,
            stream_id: stream_details.id,
            stream_created_at_micros: stream_details.created_at.as_micros(),
            replication_factor,
            max_message_bytes,
            max_active_topics,
            bootstrap_timeout,
            topics: Mutex::new(HashMap::new()),
        })
    }

    pub async fn publish(
        &self,
        producer: ProducerIdentity,
        batch: WriteBatch,
    ) -> crate::Result<()> {
        producer.validate().map_err(|error| error.to_string())?;
        if producer.tenant != batch.tenant {
            return Err("batch tenant differs from producer topic".into());
        }
        let topic = self.topic(&producer).await?;
        let parts = batch
            .split_by_shard(PRODUCER_PARTITIONS)
            .map_err(|error| error.to_string())?;
        let mut prepared = Vec::with_capacity(parts.len());
        for (partition, batch) in parts {
            let payloads = encode_chunks(
                producer.clone(),
                topic.generation,
                partition,
                PRODUCER_PARTITIONS,
                batch,
                self.max_message_bytes,
            )
            .map_err(|error| error.to_string())?;
            let mut messages = Vec::with_capacity(payloads.len());
            for (sequence, payload) in payloads.into_iter().enumerate() {
                messages.push(
                    IggyMessage::builder()
                        .id(stable_message_id(partition, sequence as u64, &payload))
                        .payload(Bytes::from(payload))
                        .build()?,
                );
            }
            prepared.push((partition, messages));
        }

        // One append and exactly one explicit fsync per touched partition.
        let sends = prepared.into_iter().map(|(partition, mut messages)| {
            let topic = Arc::clone(&topic);
            async move {
                let client = &self.clients[partition as usize % self.clients.len()];
                client
                    .send_messages(
                        &self.stream,
                        &topic.topic,
                        &Partitioning::partition_id(partition),
                        &mut messages,
                    )
                    .await?;
                client
                    .flush_unsaved_buffer(&self.stream, &topic.topic, partition, true)
                    .await?;
                Result::<(), IggyError>::Ok(())
            }
        });
        let mut first_error = None;
        for result in join_all(sends).await {
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => {
                // Topic retirement deliberately changes the broker
                // generation. Drop cached numeric IDs on any publication
                // failure; a replay reprovisions or revalidates the topic.
                self.topics.lock().await.remove(&producer);
                Err(error.into())
            }
            None => Ok(()),
        }
    }

    async fn topic(&self, producer: &ProducerIdentity) -> crate::Result<Arc<TopicPublisher>> {
        let cell = {
            let mut topics = self.topics.lock().await;
            if !topics.contains_key(producer) && topics.len() >= self.max_active_topics {
                return Err(format!(
                    "active producer topic limit {} reached",
                    self.max_active_topics
                )
                .into());
            }
            Arc::clone(
                topics
                    .entry(producer.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new())),
            )
        };
        cell.get_or_try_init(|| self.provision_topic(producer.clone()))
            .await
            .cloned()
    }

    async fn provision_topic(
        &self,
        producer: ProducerIdentity,
    ) -> crate::Result<Arc<TopicPublisher>> {
        let topic_name: Identifier = producer.topic.as_str().try_into()?;
        let (details, created) = match self.clients[0].get_topic(&self.stream, &topic_name).await? {
            Some(details) => (details, false),
            None => match self.clients[0]
                .create_topic(
                    &self.stream,
                    &producer.topic,
                    PRODUCER_PARTITIONS,
                    CompressionAlgorithm::None,
                    Some(self.replication_factor),
                    IggyExpiry::ServerDefault,
                    MaxTopicSize::ServerDefault,
                )
                .await
            {
                Ok(details) => (details, true),
                Err(_) => (
                    self.clients[0]
                        .get_topic(&self.stream, &topic_name)
                        .await?
                        .ok_or_else(|| {
                            format!(
                                "producer topic {} cannot be created or read",
                                producer.topic
                            )
                        })?,
                    false,
                ),
            },
        };
        if details.partitions_count != PRODUCER_PARTITIONS
            || details.replication_factor != self.replication_factor
        {
            return Err(format!(
                "producer topic {} has incompatible partitions/replication ({}/{})",
                producer.topic, details.partitions_count, details.replication_factor
            )
            .into());
        }
        let topic = Identifier::numeric(details.id)?;
        let generation = QueueGeneration {
            stream_id: self.stream_id,
            stream_created_at_micros: self.stream_created_at_micros,
            topic_id: details.id,
            topic_created_at_micros: details.created_at.as_micros(),
        };
        let registration = ProducerRegistration::new(producer, generation);
        if created {
            self.publish_registration(&topic, &registration).await?;
        } else {
            self.wait_for_registration(&topic, &registration).await?;
        }
        Ok(Arc::new(TopicPublisher { topic, generation }))
    }

    async fn publish_registration(
        &self,
        topic: &Identifier,
        registration: &ProducerRegistration,
    ) -> crate::Result<()> {
        let payload =
            Bytes::from(encode_registration(registration).map_err(|error| error.to_string())?);
        for partition in 0..PRODUCER_PARTITIONS {
            let mut messages = (0..2)
                .map(|sequence| {
                    IggyMessage::builder()
                        .id(registration_message_id(partition, sequence))
                        .payload(payload.clone())
                        .build()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let client = &self.clients[partition as usize % self.clients.len()];
            client
                .send_messages(
                    &self.stream,
                    topic,
                    &Partitioning::partition_id(partition),
                    &mut messages,
                )
                .await?;
            client
                .flush_unsaved_buffer(&self.stream, topic, partition, true)
                .await?;
        }
        self.validate_registration(topic, registration).await
    }

    async fn wait_for_registration(
        &self,
        topic: &Identifier,
        registration: &ProducerRegistration,
    ) -> crate::Result<()> {
        let deadline = Instant::now() + self.bootstrap_timeout;
        loop {
            match self.validate_registration(topic, registration).await {
                Ok(()) => return Ok(()),
                Err(error) if Instant::now() < deadline => {
                    tracing::debug!(message = "Waiting for concurrent Iggy topic bootstrap.", %error);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn validate_registration(
        &self,
        topic: &Identifier,
        expected: &ProducerRegistration,
    ) -> crate::Result<()> {
        for partition in 0..PRODUCER_PARTITIONS {
            let polled = self.clients[0]
                .poll_messages(
                    &self.stream,
                    topic,
                    Some(partition),
                    &Consumer::default(),
                    &PollingStrategy::offset(0),
                    2,
                    false,
                )
                .await?;
            if polled.messages.len() != 2 || polled.count != 2 {
                return Err(format!(
                    "producer topic partition {partition} is not fully bootstrapped"
                )
                .into());
            }
            for (sequence, message) in polled.messages.iter().enumerate() {
                if message.header.offset != sequence as u64
                    || message.header.id != registration_message_id(partition, sequence as u64)
                    || decode_registration(&message.payload).map_err(|error| error.to_string())?
                        != *expected
                {
                    return Err(format!(
                        "producer topic partition {partition} has a foreign registration"
                    )
                    .into());
                }
            }
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
