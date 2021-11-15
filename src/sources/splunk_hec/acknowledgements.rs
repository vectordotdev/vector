use futures_util::future::Shared;
use serde::{Deserialize, Serialize};
use tokio::{sync::{RwLock}, time::interval};
use vector_core::event::BatchStatusReceiver;
use warp::Rejection;
use std::{collections::HashMap, num::NonZeroU64, sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant}};
use roaring::RoaringTreemap;

use crate::sources::util::finalizer::OrderedFinalizer;
use crate::shutdown::ShutdownSignal;

use super::ApiError;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default)]
pub struct HecAcknowledgementsConfig {
    max_pending_acks: NonZeroU64,
    max_number_of_ack_channels: NonZeroU64,
    pub max_pending_acks_per_channel: NonZeroU64,
    ack_idle_cleanup: bool,
    max_idle_time: NonZeroU64,
}

impl Default for HecAcknowledgementsConfig {
    fn default() -> Self {
        Self {
            max_pending_acks: NonZeroU64::new(10_000_000).unwrap(),
            max_number_of_ack_channels: NonZeroU64::new(1_000_000).unwrap(),
            max_pending_acks_per_channel: NonZeroU64::new(1_000_000).unwrap(),
            ack_idle_cleanup: false,
            max_idle_time: NonZeroU64::new(300).unwrap(),
        }
    }
}

pub struct IndexerAcknowledgement {
    max_pending_acks: u64,
    max_pending_acks_per_channel: u64,
    max_number_of_ack_channels: u64,
    channels: Arc<RwLock<HashMap<String, Arc<Channel>>>>, 
    shutdown: Shared<ShutdownSignal>,
}

impl IndexerAcknowledgement {
    pub fn new(config: HecAcknowledgementsConfig, shutdown: Shared<ShutdownSignal>) -> Self {
        let channels: Arc<RwLock<HashMap<String, Arc<Channel>>>> = Arc::new(RwLock::new(HashMap::new()));
        let max_idle_time = u64::from(config.max_idle_time);
        let idle_task_channels = channels.clone();

        if config.ack_idle_cleanup {
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(max_idle_time));
                loop {
                    interval.tick().await;
                    let mut channels = idle_task_channels.write().await;
                    let expiring_channels = channels.iter().filter_map(|(channel_id, channel)| {
                        if channel.get_last_used().elapsed().as_secs() >= max_idle_time {
                            Some(channel_id.clone())
                        } else {
                            None
                        }
                    }).collect::<Vec<String>>();

                    for channel_id in expiring_channels {
                        channels.remove(&channel_id);
                    }
                }
            });
        }

        Self {
            max_pending_acks: u64::from(config.max_pending_acks),
            max_pending_acks_per_channel: u64::from(config.max_pending_acks_per_channel),
            max_number_of_ack_channels: u64::from(config.max_number_of_ack_channels),
            channels,
            shutdown,
        }
    }

    /// Creates a channel with the specified id if it does not exist.
    async fn create_or_get_channel(&self, id: String) -> Result<Arc<Channel>, Rejection> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&id) {
            return Ok(Arc::clone(channel));
        }
        drop(channels);

        // Create the channel if it does not exist
        let channel = Arc::new(Channel::new(self.max_pending_acks_per_channel, self.shutdown.clone()));
        let mut channels = self.channels.write().await;
        if channels.len() < self.max_number_of_ack_channels as usize {
            channels.insert(id, Arc::clone(&channel));
            Ok(channel)
        } else {
            Err(Rejection::from(ApiError::BadRequest))
        }
    }

    /// Gets the next available ack id from a specified channel, creating the channel if it does not exist
    pub async fn get_ack_id_from_channel(&self, channel_id: String, batch_rx: BatchStatusReceiver) -> Result<u64, Rejection> {
        let channel = self.create_or_get_channel(channel_id).await?;
        Ok(channel.get_ack_id(batch_rx))
    }

    /// Gets the requested ack id statuses from a specified channel, creating the channel if it does not exist
    pub async fn get_acks_status(&self, channel_id: String, ack_ids: &Vec<u64>) -> Result<HashMap<u64, bool>, Rejection>{
        let channel = self.create_or_get_channel(channel_id).await?;
        Ok(channel.get_acks_status(ack_ids))
    }
}

pub struct Channel {
    last_used_timestamp: Mutex<Instant>,
    currently_available_ack_id: AtomicU64,
    ack_ids_status: Arc<Mutex<RoaringTreemap>>,
    ack_event_finalizer: Arc<OrderedFinalizer<u64>>,
}

impl Channel {
    fn new(max_pending_acks_per_channel: u64, shutdown: Shared<ShutdownSignal>) -> Self {
        let ack_ids_status = Arc::new(Mutex::new(RoaringTreemap::new()));
        let finalizer_ack_ids_status= ack_ids_status.clone();
        let ack_event_finalizer = Arc::new(OrderedFinalizer::new(shutdown, move |ack_id: u64| {
            let mut ack_ids_status = finalizer_ack_ids_status.lock().unwrap();
            ack_ids_status.insert(ack_id);
            if ack_ids_status.len() > max_pending_acks_per_channel {
                match ack_ids_status.min() {
                    Some(min) => ack_ids_status.remove(min),
                    None => unreachable!(),
                };
            };
        }));

        Self {
            last_used_timestamp: Mutex::new(Instant::now()),
            currently_available_ack_id: AtomicU64::new(0),
            ack_ids_status,
            ack_event_finalizer,
        }
    }

    fn get_ack_id(&self, batch_rx: BatchStatusReceiver) -> u64 {
        {
            let mut last_used_timestamp = self.last_used_timestamp.lock().unwrap();
            *last_used_timestamp = Instant::now();
        }
        let ack_id = self.currently_available_ack_id.fetch_add(1, Ordering::Relaxed);
        self.ack_event_finalizer.add(ack_id, batch_rx);
        ack_id
    }

    fn get_acks_status(&self, acks: &Vec<u64>) -> HashMap<u64, bool> {
        {
            let mut last_used_timestamp = self.last_used_timestamp.lock().unwrap();
            *last_used_timestamp = Instant::now();
        }
        let mut ack_ids_status = self.ack_ids_status.lock().unwrap();
        acks.iter().map(|ack_id| (*ack_id, ack_ids_status.remove(*ack_id))).collect()
    }

    fn get_last_used(&self) -> Instant {
        let last_used_timestamp = self.last_used_timestamp.lock().unwrap();
        last_used_timestamp.clone()
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
    use tokio::time::sleep;

    use futures_util::FutureExt;
    use tokio::time;
    use vector_core::event::BatchNotifier;

    use crate::{shutdown::ShutdownSignal, sources::splunk_hec::acknowledgements::{Channel, HecAcknowledgementsConfig}};

    use super::IndexerAcknowledgement;

    #[tokio::test]
    async fn test_channel_get_ack_id_and_acks_status() {
        let shutdown = ShutdownSignal::noop().shared();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        for expected_ack_id in &expected_ack_ids {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*expected_ack_id, channel.get_ack_id(batch_rx));
        }
        // Allow the ack finalizer task to run
        sleep(time::Duration::from_secs(1)).await;
        assert!(channel.get_acks_status(&expected_ack_ids).values().all(|status| *status));
    }

    #[tokio::test]
    async fn test_channel_get_acks_status_repeat() {
        let shutdown = ShutdownSignal::noop().shared();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        for expected_ack_id in &expected_ack_ids {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*expected_ack_id, channel.get_ack_id(batch_rx));
        }
        // Allow the ack finalizer task to run
        sleep(time::Duration::from_secs(1)).await;
        assert!(channel.get_acks_status(&expected_ack_ids).values().all(|status| *status));
        // Subsequent queries for the same ackId's should result in false
        assert!(!channel.get_acks_status(&expected_ack_ids).values().all(|status| *status));
    }

    #[tokio::test]
    async fn test_channel_get_ack_id_exceed_max_pending_acks_per_channel() {
        let shutdown = ShutdownSignal::noop().shared();
        let max_pending_acks_per_channel = 10;
        let channel = Channel::new(max_pending_acks_per_channel, shutdown);
        let dropped_pending_ack_ids: Vec<u64> = (0..10).collect();
        let expected_ack_ids: Vec<u64> = (10..20).collect();

        for ack_id in dropped_pending_ack_ids.iter().chain(expected_ack_ids.iter()) {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*ack_id, channel.get_ack_id(batch_rx));
        }
        // Allow the ack finalizer task to run
        sleep(time::Duration::from_secs(1)).await;
        assert!(!channel.get_acks_status(&dropped_pending_ack_ids).values().all(|status| *status));
        assert!(channel.get_acks_status(&expected_ack_ids).values().all(|status| *status));
    }

    #[tokio::test]
    async fn test_indexer_ack_create_channels() {
        let shutdown = ShutdownSignal::noop().shared();
        let config = HecAcknowledgementsConfig::default();
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);

        let channel_one = idx_ack.create_or_get_channel(String::from("channel-id-1")).await.unwrap();
        let channel_two = idx_ack.create_or_get_channel(String::from("channel-id-2")).await.unwrap();

        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        let channel_one_ack_id = channel_one.get_ack_id(batch_rx);
        drop(_tx);
        let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
        let channel_two_ack_id= channel_two.get_ack_id(batch_rx);
        drop(_tx);

        assert_eq!(0, channel_one_ack_id);
        assert_eq!(0, channel_two_ack_id);
    }

    #[tokio::test]
    async fn test_indexer_ack_create_channels_exceed_max_number_of_ack_channels() {
        let shutdown = ShutdownSignal::noop().shared();
        let config = HecAcknowledgementsConfig {
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);

        let _channel_one = idx_ack.create_or_get_channel(String::from("channel-id-1")).await.unwrap();

        assert!(idx_ack.create_or_get_channel(String::from("channel-id-2")).await.is_err());
    }

    #[tokio::test]
    async fn test_indexer_ack_channel_idle_expiration() {
        let shutdown = ShutdownSignal::noop().shared();
        let config = HecAcknowledgementsConfig {
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ack_idle_cleanup: true,
            max_idle_time: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let _channel = idx_ack.create_or_get_channel(String::from("channel-id-1")).await.unwrap();
        // Allow channel to expire and free up the max channel limit of 1
        sleep(time::Duration::from_secs(2)).await;
        assert!(idx_ack.create_or_get_channel(String::from("channel-id-2")).await.is_ok());
    }

    #[tokio::test]
    async fn test_indexer_ack_channel_idle_does_not_expire_active() {
        let shutdown = ShutdownSignal::noop().shared();
        let config = HecAcknowledgementsConfig {
            ack_idle_cleanup: true,
            max_idle_time: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let idx_ack = IndexerAcknowledgement::new(config, shutdown);
        let expected_ack_ids: Vec<u64> = (0..10).collect();

        let channel = idx_ack.create_or_get_channel(String::from("channel-id-1")).await.unwrap();
        for expected_ack_id in &expected_ack_ids {
            let (_tx, batch_rx) = BatchNotifier::new_with_receiver();
            assert_eq!(*expected_ack_id, channel.get_ack_id(batch_rx));
        }
        sleep(time::Duration::from_secs(2)).await;
        assert!(channel.get_acks_status(&expected_ack_ids).values().all(|status| *status));
    }
}