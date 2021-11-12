use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};
use roaring::RoaringTreemap;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default)]
pub struct HecAcknowledgementsConfig {
    max_pending_acks: u64,
    max_number_of_ack_channels: u64,
    pub max_pending_acks_per_channel: u64,
    ack_idle_cleanup: bool,
    max_idle_time: u64,
}

impl Default for HecAcknowledgementsConfig {
    fn default() -> Self {
        Self {
            max_pending_acks: 10_000_000,
            max_number_of_ack_channels: 1_000_000,
            max_pending_acks_per_channel: 1_000_000,
            ack_idle_cleanup: false,
            max_idle_time: 300,
        }
    }
}

pub struct Channel {
    last_used_timestamp: SystemTime,
    ack_info: HecAckInfo,
}

impl Channel {
    pub fn new(max_pending_acks_per_channel: u64) -> Self {
        Self {
            last_used_timestamp: SystemTime::now(),
            ack_info: HecAckInfo::new(max_pending_acks_per_channel)
        }
    }

    pub fn get_ack_id(&mut self) -> u64 {
        self.last_used_timestamp = SystemTime::now();
        self.ack_info.get_ack_id()
    }

    pub fn get_acks_status(&mut self, acks: Vec<u64>) -> HashMap<u64, bool> {
        self.last_used_timestamp = SystemTime::now();
        self.ack_info.get_acks_status(acks)
    }
}

pub struct HecAckInfo {
    max_pending_acks_per_channel: u64,
    currently_available_ack_id: u64,
    ack_ids_in_use: RoaringTreemap,
    ack_ids_ack_status: RoaringTreemap,
}

impl HecAckInfo {
    pub fn new(max_pending_acks_per_channel: u64) -> Self {
        Self {
            max_pending_acks_per_channel,
            currently_available_ack_id: 0,
            ack_ids_in_use: RoaringTreemap::new(),
            ack_ids_ack_status: RoaringTreemap::new(),
        }
    }

    fn get_ack_id(&mut self) -> u64 {
        let ack_id = self.currently_available_ack_id;
        self.currently_available_ack_id += 1;
        self.ack_ids_in_use.insert(ack_id);
        if self.ack_ids_in_use.len() > self.max_pending_acks_per_channel {
            match self.ack_ids_in_use.min() {
                Some(oldest_ack_id) => {
                    self.ack_ids_in_use.remove(oldest_ack_id);
                    self.ack_ids_ack_status.remove(oldest_ack_id);
                },
                None => panic!("max_pending_acks_per_channel is 0"),
            }
        }
        ack_id
    }

    fn get_acks_status(&self, acks: Vec<u64>) -> HashMap<u64, bool> {
        acks.iter().map(|ack_id| (*ack_id, self.ack_ids_ack_status.contains(*ack_id))).collect()
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