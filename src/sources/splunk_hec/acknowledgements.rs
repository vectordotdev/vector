use futures::channel::mpsc;
use futures_util::future::Shared;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock};
use vector_core::event::BatchStatusReceiver;
use warp::Rejection;
use std::{collections::HashMap, num::NonZeroU64, sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}}, time::SystemTime};
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
    // channels: Arc<RwLock<HashMap<String, Arc<RwLock<Channel>>>>>, 
    channels: Arc<RwLock<HashMap<String, Arc<Channel>>>>, 
    shutdown: Shared<ShutdownSignal>,
}

impl IndexerAcknowledgement {
    pub fn new(config: HecAcknowledgementsConfig, shutdown: Shared<ShutdownSignal>) -> Self {
        Self {
            max_pending_acks: u64::from(config.max_pending_acks),
            max_pending_acks_per_channel: u64::from(config.max_pending_acks_per_channel),
            max_number_of_ack_channels: u64::from(config.max_number_of_ack_channels),
            channels: Arc::new(RwLock::new(HashMap::new())),
            shutdown,
        }
    }
    // pub async fn get_channel(&mut self, id: String) -> Result<Arc<RwLock<Channel>>, Rejection> {
    pub async fn get_channel(&self, id: String) -> Result<Arc<Channel>, Rejection> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&id) {
            return Ok(Arc::clone(channel));
        }
        drop(channels);
        // Create the channel if it does not exist
        // let channel = Arc::new(RwLock::new(Channel::new(self.max_pending_acks_per_channel)));
        let channel = Arc::new(Channel::new(self.max_pending_acks_per_channel, self.shutdown.clone()));
        let mut channels = self.channels.write().await;
        if channels.len() < self.max_number_of_ack_channels as usize {
            channels.insert(id, Arc::clone(&channel));
            return Ok(channel);
        } else {
            Err(Rejection::from(ApiError::BadRequest))
        }
    }
}

pub struct Channel {
    last_used_timestamp: Mutex<SystemTime>,
    max_pending_acks_per_channel: u64,
    // ack_info: HecAckInfo,
    currently_available_ack_id: AtomicU64,
    // ack_ids_in_use: RoaringTreemap,
    ack_ids_status: Arc<Mutex<RoaringTreemap>>,
    ack_event_finalizer: Arc<OrderedFinalizer<u64>>,
}

impl Channel {
    pub fn new(max_pending_acks_per_channel: u64, shutdown: Shared<ShutdownSignal>) -> Self {
        // let ack_ids_in_use = Arc::new(Mutex::new(RoaringTreemap::new()));
        let ack_ids_status = Arc::new(Mutex::new(RoaringTreemap::new()));
        // let (ack_tx, ack_rx) = mpsc::channel(max_pending_acks_per_channel);
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
            last_used_timestamp: Mutex::new(SystemTime::now()),
            max_pending_acks_per_channel,
            currently_available_ack_id: AtomicU64::new(0),
            // ack_ids_in_use,
            ack_ids_status,
            // ack_info: HecAckInfo::new(max_pending_acks_per_channel)
            ack_event_finalizer,
        }
    }

    pub fn get_ack_id(&self, batch_rx: BatchStatusReceiver) -> u64 {
        {
            let mut last_used_timestamp = self.last_used_timestamp.lock().unwrap();
            *last_used_timestamp = SystemTime::now();
        }
        // let ack_id = self.currently_available_ack_id.fetch_add(1, Ordering::Relaxed);
        // self.currently_available_ack_id += 1;
        // self.ack_ids_in_use.insert(ack_id);
        // if self.ack_ids_in_use.len() > self.max_pending_acks_per_channel {
        //     match self.ack_ids_in_use.min() {
        //         Some(oldest_ack_id) => {
        //             self.ack_ids_in_use.remove(oldest_ack_id);
        //             self.ack_ids_status.remove(oldest_ack_id);
        //         },
        //         None => panic!("max_pending_acks_per_channel is 0"),
        //     }
        // }
        let ack_id = self.currently_available_ack_id.fetch_add(1, Ordering::Relaxed);
        self.ack_event_finalizer.add(ack_id, batch_rx);
        ack_id
    }

    pub fn get_acks_status(&self, acks: Vec<u64>) -> HashMap<u64, bool> {
        // self.last_used_timestamp = SystemTime::now();
        {
            let mut last_used_timestamp = self.last_used_timestamp.lock().unwrap();
            *last_used_timestamp = SystemTime::now();
        }
        let mut ack_ids_status = self.ack_ids_status.lock().unwrap();
        acks.iter().map(|ack_id| (*ack_id, ack_ids_status.remove(*ack_id))).collect()
    }
}

// pub struct HecAckInfo {
//     max_pending_acks_per_channel: u64,
//     currently_available_ack_id: u64,
//     ack_ids_in_use: RoaringTreemap,
//     ack_ids_ack_status: RoaringTreemap,
// }

// impl HecAckInfo {
//     pub fn new(max_pending_acks_per_channel: u64) -> Self {
//         Self {
//             max_pending_acks_per_channel,
//             currently_available_ack_id: 0,
//             ack_ids_in_use: RoaringTreemap::new(),
//             ack_ids_ack_status: RoaringTreemap::new(),
//         }
//     }

//     fn get_ack_id(&mut self) -> u64 {
//         let ack_id = self.currently_available_ack_id;
//         self.currently_available_ack_id += 1;
//         self.ack_ids_in_use.insert(ack_id);
//         if self.ack_ids_in_use.len() > self.max_pending_acks_per_channel {
//             match self.ack_ids_in_use.min() {
//                 Some(oldest_ack_id) => {
//                     self.ack_ids_in_use.remove(oldest_ack_id);
//                     self.ack_ids_ack_status.remove(oldest_ack_id);
//                 },
//                 None => panic!("max_pending_acks_per_channel is 0"),
//             }
//         }
//         ack_id
//     }

//     fn get_acks_status(&self, acks: Vec<u64>) -> HashMap<u64, bool> {
//         acks.iter().map(|ack_id| (*ack_id, self.ack_ids_ack_status.contains(*ack_id))).collect()
//     }
// }

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
    #[tokio::test]
    async fn test() {

    }

}