use std::collections::HashMap;

use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};

/// Type of mongo instance.
/// Can be determined with `isMaster` command, see `CommandIsMaster`.
#[derive(Debug, PartialEq, Eq)]
pub enum NodeType {
    Mongod,  // MongoDB daemon
    Mongos,  // Mongo sharding server
    Replset, // <https://docs.mongodb.com/manual/reference/glossary/#term-replica-set>
}

/// <https://docs.mongodb.com/manual/reference/command/isMaster/>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandIsMaster {
    pub msg: Option<String>,
    pub set_name: Option<String>,
    pub hosts: Option<Vec<String>>,
}

/// <https://docs.mongodb.com/manual/reference/command/buildInfo/>
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandBuildInfo {
    pub version: String,
    pub git_version: String,
    pub bits: i64,
    pub debug: bool,
    pub max_bson_object_size: i64,
}

/// <https://docs.mongodb.com/manual/reference/command/serverStatus/>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatus {
    #[serde(flatten)]
    pub instance: CommandServerStatusInstance,
    pub asserts: CommandServerStatusAsserts,
    pub connections: CommandServerStatusConnections,
    #[serde(rename = "extra_info")]
    pub extra_info: CommandServerStatusExtraInfo,
    #[serde(rename = "mem")]
    pub memory: CommandServerStatusMem,
    pub global_lock: CommandServerStatusGlobalLock,
    pub locks: HashMap<String, CommandServerStatusLock>,
    pub metrics: CommandServerStatusMetrics,
    pub op_latencies: HashMap<String, CommandServerStatusOpLatenciesStat>,
    pub storage_engine: CommandServerStatusStorageEngine,
    pub wired_tiger: Option<CommandServerStatusWiredTiger>,
    pub network: CommandServerStatusNetwork,
    pub opcounters: HashMap<String, i64>,
    pub opcounters_repl: HashMap<String, i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusInstance {
    pub uptime: f64,
    pub uptime_estimate: i64,
    pub local_time: DateTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusAsserts {
    pub regular: i64,
    pub warning: i64,
    pub msg: i64,
    pub user: i64,
    pub rollovers: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusConnections {
    pub active: i64,
    pub available: i64,
    pub current: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusExtraInfo {
    pub heap_usage_bytes: Option<i64>,
    pub page_faults: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMem {
    pub resident: i64,
    pub r#virtual: i64,
    pub mapped: Option<i64>,
    pub mapped_with_journal: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusGlobalLock {
    pub total_time: i64,
    pub active_clients: CommandServerStatusGlobalLockInner,
    pub current_queue: CommandServerStatusGlobalLockInner,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusGlobalLockInner {
    pub total: i64,
    pub readers: i64,
    pub writers: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusLock {
    pub time_acquiring_micros: Option<CommandServerStatusLockModes>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusLockModes {
    #[serde(rename = "r")]
    pub read: Option<i64>,
    #[serde(rename = "w")]
    pub write: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetrics {
    pub cursor: CommandServerStatusMetricsCursor,
    pub document: CommandServerStatusMetricsDocument,
    pub get_last_error: CommandServerStatusMetricsGetLastError,
    pub operation: CommandServerStatusMetricsOperation,
    pub query_executor: CommandServerStatusMetricsQueryExecutor,
    pub record: Option<CommandServerStatusMetricsRecord>,
    pub repl: CommandServerStatusMetricsRepl,
    pub ttl: CommandServerStatusMetricsReplTtl,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsCursor {
    #[serde(rename = "timedOut")]
    pub timed_out: i64,
    pub open: CommandServerStatusMetricsCursorOpen,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsCursorOpen {
    pub no_timeout: i64,
    pub pinned: i64,
    pub total: i64,
    // Only mongos
    // pub single_target: Option<i64>,
    // pub multi_target: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsDocument {
    pub deleted: i64,
    pub inserted: i64,
    pub returned: i64,
    pub updated: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsGetLastError {
    pub wtime: CommandServerStatusMetricsGetLastErrorWtime,
    pub wtimeouts: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsGetLastErrorWtime {
    pub num: i64,
    pub total_millis: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsOperation {
    pub scan_and_order: i64,
    pub write_conflicts: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsQueryExecutor {
    pub scanned: i64,
    pub scanned_objects: i64,
    pub collection_scans: Option<CommandServerStatusMetricsQueryExecutorCollections>,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsQueryExecutorCollections {
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsRecord {
    pub moves: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsRepl {
    pub apply: CommandServerStatusMetricsReplApply,
    pub buffer: CommandServerStatusMetricsReplBuffer,
    pub executor: CommandServerStatusMetricsReplExecutor,
    pub network: CommandServerStatusMetricsReplNetwork,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsReplApply {
    pub batches: CommandServerStatusMetricsReplApplyBatches,
    pub ops: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplApplyBatches {
    pub num: i64,
    pub total_millis: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplBuffer {
    pub count: i64,
    pub max_size_bytes: i64,
    pub size_bytes: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplExecutor {
    pub queues: CommandServerStatusMetricsReplExecutorQueues,
    pub unsignaled_events: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplExecutorQueues {
    pub network_in_progress: i64,
    pub sleepers: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplNetwork {
    pub bytes: i64,
    pub getmores: CommandServerStatusMetricsReplNetworkGetmores,
    pub ops: i64,
    pub readers_created: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplNetworkGetmores {
    pub num: i64,
    pub total_millis: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplTtl {
    pub deleted_documents: i64,
    pub passes: i64,
}

/// <https://docs.mongodb.com/manual/reference/operator/aggregation/collStats/#latency-stats-document>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusOpLatenciesStat {
    pub latency: i64,
    pub ops: i64,
    pub histogram: Vec<CommandServerStatusOpLatenciesStatHistBucket>,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusOpLatenciesStatHistBucket {
    pub(crate) micros: i64,
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusStorageEngine {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusWiredTiger {
    #[serde(rename = "block-manager")]
    pub block_manager: CommandServerStatusWiredTigerBlockManager,
    pub cache: CommandServerStatusWiredTigerCache,
    pub concurrent_transactions: CommandServerStatusWiredTigerConcurrentTransactions,
    pub log: CommandServerStatusWiredTigerLog,
    pub session: CommandServerStatusWiredTigerSession,
    pub transaction: CommandServerStatusWiredTigerTransaction,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerBlockManager {
    #[serde(rename = "blocks pre-loaded")]
    pub blocks_pre_loaded: i64,
    #[serde(rename = "blocks read")]
    pub blocks_read: i64,
    #[serde(rename = "blocks written")]
    pub blocks_written: i64,
    #[serde(rename = "bytes read")]
    pub bytes_read: i64,
    #[serde(rename = "bytes written")]
    pub bytes_written: i64,
    #[serde(rename = "mapped blocks read")]
    pub mapped_blocks_read: i64,
    #[serde(rename = "mapped bytes read")]
    pub mapped_bytes_read: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerCache {
    #[serde(rename = "bytes currently in the cache")]
    pub bytes_total: i64,
    #[serde(rename = "bytes read into cache")]
    pub bytes_read_into: i64,
    #[serde(rename = "bytes written from cache")]
    pub bytes_written_from: i64,
    #[serde(rename = "maximum bytes configured")]
    pub max_bytes: f64,
    #[serde(rename = "modified pages evicted")]
    pub evicted_modified: i64,
    #[serde(rename = "pages currently held in the cache")]
    pub pages_total: i64,
    #[serde(rename = "pages read into cache")]
    pub pages_read_into: i64,
    #[serde(rename = "pages written from cache")]
    pub pages_written_from: i64,
    #[serde(rename = "percentage overhead")]
    pub percent_overhead: i64,
    #[serde(rename = "tracked bytes belonging to internal pages in the cache")]
    pub bytes_internal_pages: i64,
    #[serde(rename = "tracked bytes belonging to leaf pages in the cache")]
    pub bytes_leaf_pages: i64,
    #[serde(rename = "tracked dirty bytes in the cache")]
    pub bytes_dirty: i64,
    #[serde(rename = "tracked dirty pages in the cache")]
    pub pages_dirty: i64,
    #[serde(rename = "unmodified pages evicted")]
    pub evicted_unmodified: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerConcurrentTransactions {
    pub write: CommandServerStatusWiredTigerConcurrentTransactionsStats,
    pub read: CommandServerStatusWiredTigerConcurrentTransactionsStats,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusWiredTigerConcurrentTransactionsStats {
    pub out: i64,
    pub available: i64,
    pub total_tickets: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerLog {
    #[serde(rename = "log bytes of payload data")]
    pub bytes_payload_data: i64,
    #[serde(rename = "log bytes written")]
    pub bytes_written: i64,
    #[serde(rename = "log flush operations")]
    pub log_flushes: i64,
    #[serde(rename = "log records compressed")]
    pub records_compressed: i64,
    #[serde(rename = "log records not compressed")]
    pub records_uncompressed: i64,
    #[serde(rename = "log scan operations")]
    pub log_scans: i64,
    #[serde(rename = "log scan records requiring two reads")]
    pub log_scans_double: i64,
    #[serde(rename = "log sync operations")]
    pub log_syncs: i64,
    #[serde(rename = "log sync_dir operations")]
    pub log_sync_dirs: i64,
    #[serde(rename = "log write operations")]
    pub log_writes: i64,
    #[serde(rename = "records processed by log scan")]
    pub records_processed_log_scan: i64,
    #[serde(rename = "total log buffer size")]
    pub total_buffer_size: i64,
    #[serde(rename = "total size of compressed records")]
    pub total_size_compressed: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerSession {
    #[serde(rename = "open session count")]
    pub sessions: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerTransaction {
    #[serde(rename = "transaction begins")]
    pub begins: i64,
    #[serde(rename = "transaction checkpoints")]
    pub checkpoints: i64,
    #[serde(rename = "transaction checkpoint currently running")]
    pub checkpoints_running: i64,
    #[serde(rename = "transaction checkpoint max time (msecs)")]
    pub checkpoint_max_ms: i64,
    #[serde(rename = "transaction checkpoint min time (msecs)")]
    pub checkpoint_min_ms: i64,
    #[serde(rename = "transaction checkpoint most recent time (msecs)")]
    pub checkpoint_last_ms: i64,
    #[serde(rename = "transaction checkpoint total time (msecs)")]
    pub checkpoint_total_ms: i64,
    #[serde(rename = "transactions committed")]
    pub committed: i64,
    #[serde(rename = "transactions rolled back")]
    pub rolled_back: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusNetwork {
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub num_requests: i64,
}
