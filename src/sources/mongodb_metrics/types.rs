use mongodb::bson::DateTime;
use serde::Deserialize;

/// Type of mongo instance.
/// Can be determined with `isMaster` command, see `CommandIsMaster`.
#[derive(Debug, PartialEq, Eq)]
pub enum NodeType {
    Mongod,  // MongoDB daemon
    Mongos,  // Mongo sharding server
    Replset, // https://docs.mongodb.com/manual/reference/glossary/#term-replica-set
}

/// https://docs.mongodb.com/manual/reference/command/isMaster/
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandIsMaster {
    pub msg: Option<String>,
    pub set_name: Option<String>,
    pub hosts: Option<String>,
}

/// https://docs.mongodb.com/manual/reference/command/serverStatus/
#[derive(Debug, Deserialize)]
pub struct CommandServerStatus {
    #[serde(flatten)]
    pub instance: CommandServerStatusInstance,
    pub asserts: CommandServerStatusAsserts,
    pub connections: CommandServerStatusConnections,
    #[serde(rename = "extra_info")]
    pub extra_info: CommandServerStatusExtraInfo,
    #[serde(rename = "mem")]
    pub memory: CommandServerStatusMem,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusInstance {
    pub uptime: f64,
    pub uptime_estimate: f64,
    pub local_time: DateTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusAsserts {
    pub regular: i32,
    pub warning: i32,
    pub msg: i32,
    pub user: i32,
    pub rollovers: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusConnections {
    pub active: i32,
    pub available: i32,
    pub current: i32,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusExtraInfo {
    pub heap_usage_bytes: Option<f64>,
    pub page_faults: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMem {
    pub resident: i32,
    pub r#virtual: i32,
    pub mapped: Option<i32>,
    pub mapped_with_journal: Option<i32>,
}
