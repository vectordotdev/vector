use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use futures::StreamExt;
use roaring::RoaringTreemap;
use serde::{Deserialize, Serialize};
use tokio::time::interval;
use vector_lib::configurable::configurable_component;
use vector_lib::{finalization::BatchStatusReceiver, finalizer::UnorderedFinalizer};
use warp::Rejection;

use super::ApiError;
use crate::{event::BatchStatus, shutdown::ShutdownSignal};

/// Acknowledgement configuration for the `splunk_hec` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct HecAcknowledgementsConfig {
    /// Enables end-to-end acknowledgements.
    pub enabled: Option<bool>,

    /// The maximum number of acknowledgement statuses pending query across all channels.
    ///
    /// Equivalent to the `max_number_of_acked_requests_pending_query` Splunk HEC setting.
    ///
    /// Minimum of `1`.
    #[configurable(metadata(docs::human_name = "Max Number of Pending Acknowledgements"))]
    pub max_pending_acks: NonZeroU64,

    /// The maximum number of Splunk HEC channels clients can use with this source.
    ///
    /// Minimum of `1`.
    #[configurable(metadata(docs::human_name = "Max Number of Acknowledgement Channels"))]
    pub max_number_of_ack_channels: NonZeroU64,

    /// The maximum number of acknowledgement statuses pending query for a single channel.
    ///
    /// Equivalent to the `max_number_of_acked_requests_pending_query_per_ack_channel` Splunk HEC setting.
    ///
    /// Minimum of `1`.
    #[configurable(metadata(
        docs::human_name = "Max Number of Pending Acknowledgements Per Channel"
    ))]
    pub max_pending_acks_per_channel: NonZeroU64,

    /// Whether or not to remove channels after idling for `max_idle_time` seconds.
    ///
    /// A channel is idling if it is not used for sending data or querying acknowledgement statuses.
    #[configurable(metadata(docs::human_name = "Acknowledgement Idle Cleanup"))]
    pub ack_idle_cleanup: bool,

    /// The amount of time, in seconds, a channel is allowed to idle before removal.
    ///
    /// Channels can potentially idle for longer than this setting but clients should not rely on such behavior.
    ///
    /// Minimum of `1`.
    pub max_idle_time: NonZeroU64,
}

impl Default for HecAcknowledgementsConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            max_pending_acks: NonZeroU64::new(10_000_000).unwrap(),
            max_number_of_ack_channels: NonZeroU64::new(1_000_000).unwrap(),
            max_pending_acks_per_channel: NonZeroU64::new(1_000_000).unwrap(),
            ack_idle_cleanup: false,
            max_idle_time: NonZeroU64::new(300).unwrap(),
        }
    }
}

impl From<bool> for HecAcknowledgementsConfig {
    fn from(enabled: bool) -> Self {
        Self {
            enabled: Some(enabled),
            ..Default::default()
        }
    }
}

pub struct IndexerAcknowledgement {
    max_pending_acks: u64,
    max_pending_acks_per_channel: u64,
    max_number_of_ack_channels: u64,
    channels: Arc<tokio::sync::Mutex<HashMap<String, Arc<Channel>>>>,
    shutdown: ShutdownSignal,
    total_pending_acks: AtomicU64,
}

impl IndexerAcknowledgement {
    pub fn new(config: HecAcknowledgementsConfig, shutdown: ShutdownSignal) -> Self {
        let channels: Arc<tokio::sync::Mutex<HashMap<String, Arc<Channel>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let max_idle_time = u64::from(config.max_idle_time);
        let idle_task_channels = Arc::clone(&channels);

        if config.ack_idle_cleanup {
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(max_idle_time));
                loop {
                    interval.tick().await;
                    let mut channels = idle_task_channels.lock().await;
                    let now = Instant::now();

                    channels.retain(|_, channel| {
                        now.duration_since(channel.get_last_used()).as_secs() <= max_idle_time
                    });
                }
            });
        }

        Self {
            max_pending_acks: u64::from(config.max_pending_acks),
            max_pending_acks_per_channel: u64::from(config.max_pending_acks_per_channel),
            max_number_of_ack_channels: u64::from(config.max_number_of_ack_channels),
            channels,
            shutdown,
            total_pending_acks: AtomicU64::new(0),
        }
    }

    /// Creates a channel with the specified id if it does not exist.
    async fn create_or_get_channel(&self, id: String) -> Result<Arc<Channel>, Rejection> {
        let mut channels = self.channels.lock().await;
        if let Some(channel) = channels.get(&id) {
            return Ok(Arc::clone(channel));
        }

        if channels.len() < self.max_number_of_ack_channels as usize {
            // Create the channel if it does not exist
            let channel = Arc::new(Channel::new(
                self.max_pending_acks_per_channel,
                self.shutdown.clone(),
            ));
            channels.insert(id, Arc::clone(&channel));
            Ok(channel)
        } else {
            Err(Rejection::from(ApiError::ServiceUnavailable))
        }
    }

    /// Gets the next available ack id from a specified channel, creating the channel if it does not exist
    pub async fn get_ack_id_from_channel(
        &self,
        channel_id: String,
        batch_rx: BatchStatusReceiver,
    ) -> Result<u64, Rejection> {
        let channel = self.create_or_get_channel(channel_id).await?;
        let total_pending_acks = self.total_pending_acks.fetch_add(1, Ordering::Relaxed) + 1;
        if total_pending_acks > self.max_pending_acks
            && !self.drop_oldest_pending_ack_from_channels().await
        {
            self.total_pending_acks.fetch_sub(1, Ordering::Relaxed);
            return Err(Rejection::from(ApiError::ServiceUnavailable));
        }

        let ack_id = channel.get_ack_id(batch_rx);
        Ok(ack_id)
    }

    /// Gets the requested ack id statuses from a specified channel, creating the channel if it does not exist
    pub async fn get_acks_status_from_channel(
        &self,
        channel_id: String,
        ack_ids: &[u64],
    ) -> Result<HashMap<u64, bool>, Rejection> {
        let channel = self.create_or_get_channel(channel_id).await?;
        let acks_status = channel.get_acks_status(ack_ids);
        let dropped_ack_count = acks_status.values().filter(|status| **status).count();
        self.total_pending_acks
            .fetch_sub(dropped_ack_count as u64, Ordering::Relaxed);
        Ok(acks_status)
    }

    /// Drops the oldest ack id (if one exists) across all channels
    async fn drop_oldest_pending_ack_from_channels(&self) -> bool {
        let channels = self.channels.lock().await;
        let mut ordered_channels = channels.values().collect::<Vec<_>>();
        ordered_channels.sort_by_key(|a| a.get_last_used());
        ordered_channels
            .iter()
            .any(|channel| channel.drop_oldest_pending_ack())
    }
}

pub struct Channel {
    last_used_timestamp: RwLock<Instant>,
    currently_available_ack_id: AtomicU64,
    ack_ids_status: Arc<Mutex<RoaringTreemap>>,
    ack_event_finalizer: UnorderedFinalizer<u64>,
}

impl Channel {
    fn new(max_pending_acks_per_channel: u64, shutdown: ShutdownSignal) -> Self {
        let ack_ids_status = Arc::new(Mutex::new(RoaringTreemap::new()));
        let finalizer_ack_ids_status = Arc::clone(&ack_ids_status);
        let (ack_event_finalizer, mut ack_stream) = UnorderedFinalizer::new(Some(shutdown));
        tokio::spawn(async move {
            while let Some((status, ack_id)) = ack_stream.next().await {
                if status == BatchStatus::Delivered {
                    let mut ack_ids_status = finalizer_ack_ids_status.lock().unwrap();
                    ack_ids_status.insert(ack_id);
                    if ack_ids_status.len() > max_pending_acks_per_channel {
                        match ack_ids_status.min() {
                            Some(min) => ack_ids_status.remove(min),
                            // max pending acks per channel is guaranteed to be >= 1,
                            // thus there must be at least one ack id available to remove
                            None => unreachable!(
                                "Indexer acknowledgements channel must allow at least one pending ack"
                            ),
                        };
                    }
                }
            }
        });

        Self {
            last_used_timestamp: RwLock::new(Instant::now()),
            currently_available_ack_id: AtomicU64::new(0),
            ack_ids_status,
            ack_event_finalizer,
        }
    }

    fn get_ack_id(&self, batch_rx: BatchStatusReceiver) -> u64 {
        {
            let mut last_used_timestamp = self.last_used_timestamp.write().unwrap();
            *last_used_timestamp = Instant::now();
        }
        let ack_id = self
            .currently_available_ack_id
            .fetch_add(1, Ordering::Relaxed);
        self.ack_event_finalizer.add(ack_id, batch_rx);
        ack_id
    }

    fn get_acks_status(&self, acks: &[u64]) -> HashMap<u64, bool> {
        {
            let mut last_used_timestamp = self.last_used_timestamp.write().unwrap();
            *last_used_timestamp = Instant::now();
        }
        let mut ack_ids_status = self.ack_ids_status.lock().unwrap();
        acks.iter()
            .map(|ack_id| (*ack_id, ack_ids_status.remove(*ack_id)))
            .collect()
    }

    fn get_last_used(&self) -> Instant {
        let last_used_timestamp = self.last_used_timestamp.read().unwrap();
        *last_used_timestamp
    }

    fn drop_oldest_pending_ack(&self) -> bool {
        let mut ack_ids_status = self.ack_ids_status.lock().unwrap();
        match ack_ids_status.min() {
            Some(ack_id) => ack_ids_status.remove(ack_id),
            None => false,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HecAckStatusRequest {
    pub acks: Vec<u64>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HecAckStatusResponse {
    pub acks: HashMap<u64, bool>,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use tokio::{time, time::sleep};
    use vector_lib::event::{BatchNotifier, EventFinalizer, EventStatus};

    use super::{Channel, HecAcknowledgementsConfig, IndexerAcknowledgement};
    use crate::shutdown::ShutdownSignal;

    #[tokio::test]
    async fn test_channel_get_ack_id_and_acks_status() {
        channel_get_ack_id_and_status(EventStatus::Delivered, true).await;
    }

    #[tokio::test]
    async fn test_channel_get_ack_id_and_nacks_status() {
        channel_get_ack_id_and_status(EventStatus::Rejected, false).await;
    }

    async fn channel_get_ack_id_and_status(status: EventStatus, result: bool) {
        let shutdown = ShutdownSignal::noop();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        for expected_ack_id in &expected_ack_ids {
            let (tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*expected_ack_id, channel.get_ack_id(batch_rx));
            EventFinalizer::new(tx).update_status(status);
        }
        // Let the ack finalizer task run
        sleep(time::Duration::from_secs(1)).await;
        assert!(channel
            .get_acks_status(&expected_ack_ids)
            .values()
            .all(|&status| status == result));
    }

    #[tokio::test]
    async fn test_channel_get_acks_status_repeat() {
        let shutdown = ShutdownSignal::noop();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        for expected_ack_id in &expected_ack_ids {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*expected_ack_id, channel.get_ack_id(batch_rx));
        }
        // Let the ack finalizer task run
        sleep(time::Duration::from_secs(1)).await;
        assert!(channel
            .get_acks_status(&expected_ack_ids)
            .values()
            .all(|status| *status));
        // Subsequent queries for the same ackId's should result in false
        assert!(channel
            .get_acks_status(&expected_ack_ids)
            .values()
            .all(|status| !*status));
    }

    #[tokio::test]
    async fn test_channel_get_ack_id_exceed_max_pending_acks_per_channel() {
        let shutdown = ShutdownSignal::noop();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let dropped_pending_ack_ids: Vec<u64> = (0..10).collect();
        let expected_ack_ids: Vec<u64> = (10..20).collect();

        for ack_id in dropped_pending_ack_ids
            .iter()
            .chain(expected_ack_ids.iter())
        {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*ack_id, channel.get_ack_id(batch_rx));
        }
        // Let the ack finalizer task run
        sleep(time::Duration::from_secs(1)).await;
        // The first 10 pending ack ids are dropped
        assert!(channel
            .get_acks_status(&dropped_pending_ack_ids)
            .values()
            .all(|status| !*status));
        // The second 10 pending ack ids can be queried
        assert!(channel
            .get_acks_status(&expected_ack_ids)
            .values()
            .all(|status| *status));
    }

    #[tokio::test]
    async fn test_indexer_ack_exceed_max_pending_acks_drop_acks() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_pending_acks: NonZeroU64::new(10).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let channel = String::from("channel-id");

        let dropped_pending_ack_ids: Vec<u64> = (0..10).collect();
        let expected_ack_ids: Vec<u64> = (10..20).collect();

        for ack_id in dropped_pending_ack_ids
            .iter()
            .chain(expected_ack_ids.iter())
        {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(
                *ack_id,
                idx_ack
                    .get_ack_id_from_channel(channel.clone(), batch_rx)
                    .await
                    .unwrap()
            );
            sleep(time::Duration::from_millis(100)).await;
        }
        sleep(time::Duration::from_secs(1)).await;
        assert!(idx_ack
            .get_acks_status_from_channel(channel.clone(), &dropped_pending_ack_ids)
            .await
            .unwrap()
            .values()
            .all(|status| !*status));
        assert!(idx_ack
            .get_acks_status_from_channel(channel, &expected_ack_ids)
            .await
            .unwrap()
            .values()
            .all(|status| *status));
    }

    #[tokio::test]
    async fn test_indexer_ack_exceed_max_pending_acks_server_busy() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_pending_acks: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let channel = String::from("channel-id");

        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        idx_ack
            .get_ack_id_from_channel(channel.clone(), batch_rx)
            .await
            .unwrap();

        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        assert!(idx_ack
            .get_ack_id_from_channel(channel.clone(), batch_rx)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_indexer_ack_create_channels() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);

        let channel_one = idx_ack
            .create_or_get_channel(String::from("channel-id-1"))
            .await
            .unwrap();
        let channel_two = idx_ack
            .create_or_get_channel(String::from("channel-id-2"))
            .await
            .unwrap();

        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        let channel_one_ack_id = channel_one.get_ack_id(batch_rx);
        drop(_tx);
        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        let channel_two_ack_id = channel_two.get_ack_id(batch_rx);
        drop(_tx);

        assert_eq!(0, channel_one_ack_id);
        assert_eq!(0, channel_two_ack_id);
    }

    #[tokio::test]
    async fn test_indexer_ack_create_channels_exceed_max_number_of_ack_channels() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);

        let _channel_one = idx_ack
            .create_or_get_channel(String::from("channel-id-1"))
            .await
            .unwrap();

        assert!(idx_ack
            .create_or_get_channel(String::from("channel-id-2"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_indexer_ack_channel_idle_expiration() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ack_idle_cleanup: true,
            max_idle_time: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let _channel = idx_ack
            .create_or_get_channel(String::from("channel-id-1"))
            .await
            .unwrap();
        // Allow channel to expire and free up the max channel limit of 1
        sleep(time::Duration::from_secs(3)).await;
        assert!(idx_ack
            .create_or_get_channel(String::from("channel-id-2"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_indexer_ack_channel_active_does_not_expire() {
        let shutdown = ShutdownSignal::noop();
        let config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ack_idle_cleanup: true,
            max_idle_time: NonZeroU64::new(2).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let channel = String::from("channel-id");
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        for expected_ack_id in &expected_ack_ids {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(
                *expected_ack_id,
                idx_ack
                    .get_ack_id_from_channel(channel.clone(), batch_rx)
                    .await
                    .unwrap()
            );
        }
        sleep(time::Duration::from_secs(2)).await;
        assert!(idx_ack
            .get_acks_status_from_channel(channel.clone(), &expected_ack_ids)
            .await
            .unwrap()
            .values()
            .all(|status| *status));
    }
}
