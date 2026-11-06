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
    /// Added in MongoDB 8.0.
    pub queues: Option<CommandServerStatusQueues>,
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

/// The layout of `metrics.repl.buffer` changed in MongoDB 8.0.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CommandServerStatusMetricsReplBuffer {
    /// MongoDB 8.0 and later: separate buffers to write and to apply oplog entries.
    V8(CommandServerStatusMetricsReplBufferV8),
    /// MongoDB 7.0 and earlier: one buffer.
    Legacy(CommandServerStatusMetricsReplBufferStats),
}

impl CommandServerStatusMetricsReplBuffer {
    /// Totals for all oplog entries that the node fetched but did not apply yet.
    pub const fn totals(&self) -> CommandServerStatusMetricsReplBufferStats {
        match self {
            Self::V8(buffer) => CommandServerStatusMetricsReplBufferStats {
                count: buffer.write.count + buffer.apply.count,
                max_size_bytes: buffer.write.max_size_bytes + buffer.apply.max_size_bytes,
                size_bytes: buffer.write.size_bytes + buffer.apply.size_bytes,
            },
            Self::Legacy(buffer) => *buffer,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusMetricsReplBufferV8 {
    pub write: CommandServerStatusMetricsReplBufferStats,
    pub apply: CommandServerStatusMetricsReplBufferStats,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusMetricsReplBufferStats {
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

/// The layout of `wiredTiger` changed in MongoDB 8.0.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CommandServerStatusWiredTiger {
    /// MongoDB 8.0 and later.
    V8(CommandServerStatusWiredTigerV8),
    /// MongoDB 7.0 and earlier.
    Legacy(CommandServerStatusWiredTigerLegacy),
}

impl CommandServerStatusWiredTiger {
    pub const fn common(&self) -> &CommandServerStatusWiredTigerCommon {
        match self {
            Self::V8(stat) => &stat.common,
            Self::Legacy(stat) => &stat.common,
        }
    }

    pub const fn transaction(&self) -> &CommandServerStatusWiredTigerTransaction {
        match self {
            Self::V8(stat) => &stat.transaction,
            Self::Legacy(stat) => &stat.transaction.common,
        }
    }

    /// Execution tickets. MongoDB 8.0 moved them to the top-level `queues.execution`.
    pub const fn tickets<'a>(
        &'a self,
        queues: Option<&'a CommandServerStatusQueues>,
    ) -> Option<&'a CommandServerStatusTickets> {
        match self {
            Self::V8(_) => match queues {
                Some(queues) => Some(&queues.execution),
                None => None,
            },
            Self::Legacy(stat) => Some(&stat.concurrent_transactions),
        }
    }

    pub fn checkpoint(&self) -> WiredTigerCheckpointStats {
        match self {
            Self::V8(stat) => WiredTigerCheckpointStats {
                started: stat.checkpoint.started_by_api + stat.checkpoint.started_by_compaction,
                running: i64::from(stat.checkpoint.progress_state != 0),
                max_ms: stat.checkpoint.max_ms,
                min_ms: stat.checkpoint.min_ms,
                total_ms: stat.checkpoint.total_ms,
            },
            Self::Legacy(stat) => WiredTigerCheckpointStats {
                started: stat.transaction.checkpoints,
                running: stat.transaction.checkpoints_running,
                max_ms: stat.transaction.checkpoint_max_ms,
                min_ms: stat.transaction.checkpoint_min_ms,
                total_ms: stat.transaction.checkpoint_total_ms,
            },
        }
    }
}

/// Checkpoint statistics for all `wiredTiger` layouts.
pub struct WiredTigerCheckpointStats {
    pub started: i64,
    pub running: i64,
    pub max_ms: i64,
    pub min_ms: i64,
    pub total_ms: i64,
}

/// MongoDB 8.0 moved the checkpoint statistics from `transaction` to `checkpoint`, and the
/// execution tickets from `concurrentTransactions` to the top-level `queues.execution`.
#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerV8 {
    #[serde(flatten)]
    pub common: CommandServerStatusWiredTigerCommon,
    pub checkpoint: CommandServerStatusWiredTigerCheckpoint,
    pub transaction: CommandServerStatusWiredTigerTransaction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusWiredTigerLegacy {
    #[serde(flatten)]
    pub common: CommandServerStatusWiredTigerCommon,
    pub concurrent_transactions: CommandServerStatusTickets,
    pub transaction: CommandServerStatusWiredTigerTransactionLegacy,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerCommon {
    #[serde(rename = "block-manager")]
    pub block_manager: CommandServerStatusWiredTigerBlockManager,
    pub cache: CommandServerStatusWiredTigerCache,
    pub log: CommandServerStatusWiredTigerLog,
    pub session: CommandServerStatusWiredTigerSession,
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

/// <https://www.mongodb.com/docs/manual/reference/command/serverStatus/#queues>
#[derive(Debug, Deserialize)]
pub struct CommandServerStatusQueues {
    pub execution: CommandServerStatusTickets,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusTickets {
    pub write: CommandServerStatusTicketsStats,
    pub read: CommandServerStatusTicketsStats,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusTicketsStats {
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
    #[serde(rename = "transactions committed")]
    pub committed: i64,
    #[serde(rename = "transactions rolled back")]
    pub rolled_back: i64,
}

#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerTransactionLegacy {
    #[serde(flatten)]
    pub common: CommandServerStatusWiredTigerTransaction,
    #[serde(rename = "transaction checkpoints")]
    pub checkpoints: i64,
    #[serde(rename = "transaction checkpoint currently running")]
    pub checkpoints_running: i64,
    #[serde(rename = "transaction checkpoint max time (msecs)")]
    pub checkpoint_max_ms: i64,
    #[serde(rename = "transaction checkpoint min time (msecs)")]
    pub checkpoint_min_ms: i64,
    #[serde(rename = "transaction checkpoint total time (msecs)")]
    pub checkpoint_total_ms: i64,
}

/// Added in MongoDB 8.0.
#[derive(Debug, Deserialize)]
pub struct CommandServerStatusWiredTigerCheckpoint {
    #[serde(rename = "number of checkpoints started by api")]
    pub started_by_api: i64,
    #[serde(rename = "number of checkpoints started by compaction")]
    pub started_by_compaction: i64,
    /// `0` when no checkpoint runs.
    #[serde(rename = "progress state")]
    pub progress_state: i64,
    #[serde(rename = "max time (msecs)")]
    pub max_ms: i64,
    #[serde(rename = "min time (msecs)")]
    pub min_ms: i64,
    #[serde(rename = "total time (msecs)")]
    pub total_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandServerStatusNetwork {
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub num_requests: i64,
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Document, doc, from_document};

    use super::*;

    fn wired_tiger_common() -> Document {
        doc! {
            "block-manager": {
                "blocks pre-loaded": 1, "blocks read": 1, "blocks written": 1, "bytes read": 1,
                "bytes written": 1, "mapped blocks read": 1, "mapped bytes read": 1,
            },
            "cache": {
                "bytes currently in the cache": 1, "maximum bytes configured": 1.0,
                "modified pages evicted": 1, "pages currently held in the cache": 1,
                "pages read into cache": 1, "pages written from cache": 1, "percentage overhead": 1,
                "tracked bytes belonging to internal pages in the cache": 1,
                "tracked bytes belonging to leaf pages in the cache": 1,
                "tracked dirty bytes in the cache": 1, "tracked dirty pages in the cache": 1,
                "unmodified pages evicted": 1,
            },
            "log": {
                "log bytes of payload data": 1, "log bytes written": 1, "log flush operations": 1,
                "log records compressed": 1, "log records not compressed": 1,
                "log scan operations": 1, "log scan records requiring two reads": 1,
                "log sync operations": 1, "log sync_dir operations": 1, "log write operations": 1,
                "records processed by log scan": 1,
            },
            "session": { "open session count": 1 },
        }
    }

    fn tickets(available: i32) -> Document {
        doc! {
            "read": { "out": 0, "available": available, "totalTickets": available },
            "write": { "out": 0, "available": available, "totalTickets": available },
        }
    }

    #[test]
    fn wired_tiger_legacy_layout() {
        let mut wired_tiger = wired_tiger_common();
        wired_tiger.insert("concurrentTransactions", tickets(128));
        wired_tiger.insert(
            "transaction",
            doc! {
                "transaction begins": 10, "transactions committed": 4,
                "transactions rolled back": 6, "transaction checkpoints": 3_i64,
                "transaction checkpoint currently running": 1,
                "transaction checkpoint max time (msecs)": 30,
                "transaction checkpoint min time (msecs)": 10,
                "transaction checkpoint total time (msecs)": 60,
            },
        );
        let wired_tiger: CommandServerStatusWiredTiger = from_document(wired_tiger).unwrap();
        assert!(matches!(
            wired_tiger,
            CommandServerStatusWiredTiger::Legacy(_)
        ));

        let checkpoint = wired_tiger.checkpoint();
        assert_eq!(checkpoint.started, 3);
        assert_eq!(checkpoint.running, 1);
        assert_eq!(
            (checkpoint.min_ms, checkpoint.max_ms, checkpoint.total_ms),
            (10, 30, 60)
        );
        assert_eq!(wired_tiger.transaction().committed, 4);

        // `queues.execution` only exists on MongoDB 8.0 and later.
        let queues: CommandServerStatusQueues =
            from_document(doc! { "execution": tickets(8) }).unwrap();
        let tickets = wired_tiger.tickets(Some(&queues)).unwrap();
        assert_eq!(tickets.read.available, 128);
    }

    #[test]
    fn wired_tiger_v8_layout() {
        let mut wired_tiger = wired_tiger_common();
        wired_tiger.insert(
            "checkpoint",
            doc! {
                "number of checkpoints started by api": 2,
                "number of checkpoints started by compaction": 1,
                "progress state": 17, "max time (msecs)": 30, "min time (msecs)": 10,
                "total time (msecs)": 60,
            },
        );
        wired_tiger.insert(
            "transaction",
            doc! {
                "transaction begins": 10, "transactions committed": 4,
                "transactions rolled back": 6,
            },
        );
        let wired_tiger: CommandServerStatusWiredTiger = from_document(wired_tiger).unwrap();
        assert!(matches!(wired_tiger, CommandServerStatusWiredTiger::V8(_)));

        let checkpoint = wired_tiger.checkpoint();
        assert_eq!(checkpoint.started, 3);
        assert_eq!(checkpoint.running, 1);
        assert_eq!(
            (checkpoint.min_ms, checkpoint.max_ms, checkpoint.total_ms),
            (10, 30, 60)
        );
        assert_eq!(wired_tiger.transaction().rolled_back, 6);

        let queues: CommandServerStatusQueues =
            from_document(doc! { "execution": tickets(8) }).unwrap();
        assert_eq!(
            wired_tiger.tickets(Some(&queues)).unwrap().write.available,
            8
        );
        assert!(wired_tiger.tickets(None).is_none());
    }

    #[test]
    fn wired_tiger_v8_checkpoint_not_running() {
        let mut wired_tiger = wired_tiger_common();
        wired_tiger.insert(
            "checkpoint",
            doc! {
                "number of checkpoints started by api": 0,
                "number of checkpoints started by compaction": 0,
                "progress state": 0, "max time (msecs)": 0, "min time (msecs)": 0,
                "total time (msecs)": 0,
            },
        );
        wired_tiger.insert(
            "transaction",
            doc! {
                "transaction begins": 0, "transactions committed": 0,
                "transactions rolled back": 0,
            },
        );
        let wired_tiger: CommandServerStatusWiredTiger = from_document(wired_tiger).unwrap();
        assert_eq!(wired_tiger.checkpoint().running, 0);
    }

    #[test]
    fn repl_buffer_layouts() {
        let legacy: CommandServerStatusMetricsReplBuffer =
            from_document(doc! { "count": 1, "maxSizeBytes": 100_i64, "sizeBytes": 10 }).unwrap();
        let totals = legacy.totals();
        assert_eq!(
            (totals.count, totals.max_size_bytes, totals.size_bytes),
            (1, 100, 10)
        );

        let v8: CommandServerStatusMetricsReplBuffer = from_document(doc! {
            "write": { "count": 1, "maxSizeBytes": 100_i64, "sizeBytes": 10 },
            "apply": { "count": 2, "maxSizeBytes": 200_i64, "sizeBytes": 20, "maxCount": 5 },
        })
        .unwrap();
        let totals = v8.totals();
        assert_eq!(
            (totals.count, totals.max_size_bytes, totals.size_bytes),
            (3, 300, 30)
        );
    }
}
