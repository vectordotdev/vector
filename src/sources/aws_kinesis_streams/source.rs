//! Main runtime for the `aws_kinesis_streams` source.
//!
//! Implements shard discovery, balanced/explicit shard assignment, per-shard polling
//! loops, and at-least-once delivery via DynamoDB checkpointing.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_kinesis::{
    Client as KinesisClient,
    error::DisplayErrorContext,
    types::{Record, Shard, ShardIteratorType},
};
use chrono::{TimeZone, Utc};
use tokio::select;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tokio_util::codec::Decoder as _;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::StreamDecodingError,
    config::LogNamespace,
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
    lookup::{PathPrefix, metadata_path},
};

use crate::{
    SourceSender,
    codecs::Decoder,
    config::log_schema,
    event::{BatchNotifier, BatchStatus, Event},
    shutdown::ShutdownSignal,
    sources::aws_kinesis_streams::{
        AwsKinesisStreamsConfig,
        batcher::SequenceTracker,
        checkpointer::{CheckpointerError, KinesisCheckpointer},
        closed_shard::{
            IteratorErrorAction, SHARD_END, ShardIteratorChoice, children_by_parent,
            is_child_shard, is_shard_closed, is_shard_end, iterator_error_action,
            leftover_checkpoint_shard_ids, next_growing_empty_polls, parent_ids,
            shard_iterator_choice, should_claim_shard, should_delete_shard_end_lease,
            should_finish_shard_consumer,
        },
    },
};

const KINESIS_MAX_RECORDS: i32 = 10_000;
const BACKOFF_MAX_MS: u64 = 5_000;
// AWS enforces a hard limit of 5 GetRecords calls per shard per second.
const GET_RECORDS_MIN_INTERVAL_MS: u64 = 200;
// Maximum configurable poll interval, matching the KCL idleTimeBetweenReadsInMillis default.
const POLL_INTERVAL_MAX_MS: u64 = 1_000;
// Per-shard read throughput budget: 2 MiB/sec sliding average.
// A GetRecords response of N bytes consumes N/2MiB seconds of read budget.
const SHARD_READ_BUDGET_BYTES_PER_SEC: u64 = 2 * 1024 * 1024;
// Backoff for ProvisionedThroughputExceededException: start at 1s (AWS recommendation), cap at 10s.
const THROTTLE_BACKOFF_INITIAL_MS: u64 = 1_000;
const THROTTLE_BACKOFF_MAX_MS: u64 = 10_000;

/// Parsed stream entry from the config `streams` field.
#[derive(Debug, Clone)]
struct StreamEntry {
    /// The stream identifier as it appears in the config (name or ARN, no shard suffix).
    id: String,
    /// Resolved ARN after `DescribeStream`.
    arn: String,
    /// If non-empty, consume only these specific shard IDs (explicit mode).
    explicit_shards: Vec<String>,
}

pub struct KinesisStreamsSource {
    config: AwsKinesisStreamsConfig,
    kinesis: KinesisClient,
    dynamodb: DynamoDbClient,
    decoder: Decoder,
    acknowledgements: bool,
    log_namespace: LogNamespace,
    client_id: String,
}

impl KinesisStreamsSource {
    pub fn new(
        config: AwsKinesisStreamsConfig,
        kinesis: KinesisClient,
        dynamodb: DynamoDbClient,
        decoder: Decoder,
        acknowledgements: bool,
        log_namespace: LogNamespace,
    ) -> crate::Result<Self> {
        Ok(Self {
            config,
            kinesis,
            dynamodb,
            decoder,
            acknowledgements,
            log_namespace,
            client_id: Uuid::new_v4().to_string(),
        })
    }

    pub async fn run(self, out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            shutdown.await;
            cancel_clone.cancel();
        });

        let streams = match self.parse_streams() {
            Ok(s) => s,
            Err(e) => {
                error!(message = "Failed to parse stream configuration.", error = %e);
                return Err(());
            }
        };

        let explicit_mode = streams.iter().any(|s| !s.explicit_shards.is_empty());

        let checkpointer = match KinesisCheckpointer::new(
            self.dynamodb.clone(),
            self.client_id.clone(),
            self.config.dynamodb.clone(),
            Duration::from_secs(self.config.lease_period_secs),
        )
        .await
        {
            Ok(c) => Arc::new(c),
            Err(e) => {
                error!(message = "Failed to initialise DynamoDB checkpointer.", error = %e);
                return Err(());
            }
        };

        let mut resolved = Vec::with_capacity(streams.len());
        for mut entry in streams {
            match self.resolve_stream_arn(&entry.id).await {
                Ok(arn) => {
                    entry.arn = arn;
                    resolved.push(entry);
                }
                Err(e) => {
                    error!(message = "Failed to resolve Kinesis stream ARN.", stream = %entry.id, error = %e);
                    return Err(());
                }
            }
        }

        let arc_self = Arc::new(self);

        if explicit_mode {
            arc_self
                .run_explicit(resolved, checkpointer, out, cancel)
                .await;
        } else {
            arc_self
                .run_balanced(resolved, checkpointer, out, cancel)
                .await;
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Stream parsing
    // -------------------------------------------------------------------------

    fn parse_streams(&self) -> crate::Result<Vec<StreamEntry>> {
        let mut balanced: Vec<StreamEntry> = Vec::new();
        let mut explicit_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut seen_balanced = false;
        let mut seen_explicit = false;

        for raw in &self.config.streams {
            for part in raw.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }

                let (id, shard) = parse_stream_id(part)?;

                if shard.is_empty() {
                    if seen_explicit {
                        return Err("Cannot mix balanced and explicit-shard streams".into());
                    }
                    seen_balanced = true;
                    balanced.push(StreamEntry {
                        id,
                        arn: String::new(),
                        explicit_shards: vec![],
                    });
                } else {
                    if seen_balanced {
                        return Err("Cannot mix balanced and explicit-shard streams".into());
                    }
                    seen_explicit = true;
                    explicit_map.entry(id).or_default().push(shard);
                }
            }
        }

        if seen_explicit {
            return Ok(explicit_map
                .into_iter()
                .map(|(id, shards)| StreamEntry {
                    id,
                    arn: String::new(),
                    explicit_shards: shards,
                })
                .collect());
        }

        Ok(balanced)
    }

    // -------------------------------------------------------------------------
    // AWS helpers
    // -------------------------------------------------------------------------

    async fn resolve_stream_arn(&self, id: &str) -> crate::Result<String> {
        if id.starts_with("arn:") {
            return Ok(id.to_string());
        }

        let result = self
            .kinesis
            .describe_stream()
            .stream_name(id)
            .send()
            .await
            .map_err(|e| {
                format!(
                    "Failed to describe Kinesis stream '{id}': {}",
                    DisplayErrorContext(&e)
                )
            })?;

        let arn = result
            .stream_description
            .map(|d| d.stream_arn().to_string())
            .ok_or_else(|| format!("No StreamARN in DescribeStream response for '{id}'"))?;

        Ok(arn)
    }

    async fn collect_shards(&self, arn: &str) -> crate::Result<Vec<Shard>> {
        let mut shards = Vec::new();
        let mut next_token: Option<String> = None;

        loop {
            let result = if let Some(token) = next_token.take() {
                self.kinesis.list_shards().next_token(token).send().await
            } else {
                self.kinesis.list_shards().stream_arn(arn).send().await
            }
            .map_err(|e| {
                format!(
                    "Failed to list shards for stream '{arn}': {}",
                    DisplayErrorContext(&e)
                )
            })?;

            if let Some(s) = result.shards {
                shards.extend(s);
            }

            match result.next_token {
                Some(t) => next_token = Some(t),
                None => break,
            }
        }

        Ok(shards)
    }

    async fn get_shard_iterator(
        &self,
        arn: &str,
        shard_id: &str,
        sequence: &str,
        start_from_oldest: bool,
        shard_closed: bool,
        is_child: bool,
    ) -> Result<String, ShardIteratorError> {
        let choice = shard_iterator_choice(sequence, start_from_oldest, shard_closed, is_child);
        let (iter_type, has_seq) = match choice {
            ShardIteratorChoice::AfterSequenceNumber => {
                (ShardIteratorType::AfterSequenceNumber, true)
            }
            ShardIteratorChoice::TrimHorizon => (ShardIteratorType::TrimHorizon, false),
            ShardIteratorChoice::Latest => (ShardIteratorType::Latest, false),
        };

        let mut req = self
            .kinesis
            .get_shard_iterator()
            .stream_arn(arn)
            .shard_id(shard_id)
            .shard_iterator_type(iter_type);

        if has_seq {
            req = req.starting_sequence_number(sequence);
        }

        let result = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let not_found = e
                    .as_service_error()
                    .map(|se| se.is_resource_not_found_exception())
                    .unwrap_or(false);
                return Err(ShardIteratorError::new(
                    not_found,
                    false,
                    DisplayErrorContext(&e),
                ));
            }
        };

        match result.shard_iterator {
            Some(iter) if !iter.is_empty() => Ok(iter),
            _ => {
                let fallback = match self
                    .kinesis
                    .get_shard_iterator()
                    .stream_arn(arn)
                    .shard_id(shard_id)
                    .shard_iterator_type(ShardIteratorType::TrimHorizon)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let not_found = e
                            .as_service_error()
                            .map(|se| se.is_resource_not_found_exception())
                            .unwrap_or(false);
                        return Err(ShardIteratorError::new(
                            not_found,
                            true,
                            DisplayErrorContext(&e),
                        ));
                    }
                };

                fallback
                    .shard_iterator
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ShardIteratorError::Other("Failed to obtain shard iterator".into())
                    })
            }
        }
    }

    // -------------------------------------------------------------------------
    // Balanced mode
    // -------------------------------------------------------------------------

    async fn run_balanced(
        self: Arc<Self>,
        streams: Vec<StreamEntry>,
        checkpointer: Arc<KinesisCheckpointer>,
        out: SourceSender,
        cancel: CancellationToken,
    ) {
        let rebalance_interval = Duration::from_secs(self.config.rebalance_period_secs);
        let lease_period = Duration::from_secs(self.config.lease_period_secs);
        let mut task_set: JoinSet<()> = JoinSet::new();

        'outer: loop {
            for stream in &streams {
                let all_shards = match self.collect_shards(&stream.arn).await {
                    Ok(s) => s,
                    Err(e) => {
                        if cancel.is_cancelled() {
                            break 'outer;
                        }
                        error!(message = "Failed to list shards.", stream = %stream.id, error = %e);
                        continue;
                    }
                };

                let checkpoint_data = match checkpointer
                    .get_checkpoints_and_claims(&stream.id)
                    .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        if cancel.is_cancelled() {
                            break 'outer;
                        }
                        error!(message = "Failed to fetch checkpoints.", stream = %stream.id, error = %e);
                        continue;
                    }
                };

                // KCL 3.0 lease eligibility: never claim SHARD_END; never claim a
                // child until its parents are SHARD_END (or have no lease); closed
                // parents are only claimed when a leftover checkpoint still needs
                // draining.
                let mut unclaimed: HashMap<String, String> = HashMap::new();
                let mut closed_by_id: HashMap<String, bool> = HashMap::new();
                let mut parent_ids_by_shard: HashMap<String, Vec<String>> = HashMap::new();
                let mut shard_parent_pairs: Vec<(String, Vec<String>)> = Vec::new();
                let mut listed_ids: HashSet<String> = HashSet::new();
                for shard in &all_shards {
                    let shard_id = shard.shard_id().to_string();
                    let closed = is_shard_closed(
                        shard
                            .sequence_number_range()
                            .and_then(|r| r.ending_sequence_number()),
                    );
                    let parents =
                        parent_ids(shard.parent_shard_id(), shard.adjacent_parent_shard_id());
                    closed_by_id.insert(shard_id.clone(), closed);
                    parent_ids_by_shard.insert(shard_id.clone(), parents.clone());
                    shard_parent_pairs.push((shard_id.clone(), parents.clone()));
                    listed_ids.insert(shard_id.clone());

                    let sequence = checkpoint_data.sequence_numbers.get(&shard_id);
                    if should_claim_shard(
                        closed,
                        checkpoint_data
                            .shards_with_checkpoints
                            .contains_key(&shard_id),
                        sequence.map(String::as_str),
                        &parents,
                        &checkpoint_data.sequence_numbers,
                    ) {
                        unclaimed.insert(shard_id, String::new());
                    }
                }

                let children_map = children_by_parent(&shard_parent_pairs);

                // Delete leftover rows for shards ListShards no longer returns
                // (retention expired) and SHARD_END parents whose children have
                // started. Skip rows with an unexpired lease so an in-flight
                // consumer is not deleted out from under itself.
                let actively_leased: HashSet<String> = checkpoint_data
                    .client_claims
                    .values()
                    .flatten()
                    .filter(|claim| {
                        Utc::now()
                            .signed_duration_since(claim.lease_timeout)
                            .to_std()
                            .unwrap_or(Duration::ZERO)
                            <= lease_period * 2
                    })
                    .map(|claim| claim.shard_id.clone())
                    .collect();

                for shard_id in leftover_checkpoint_shard_ids(
                    checkpoint_data.sequence_numbers.keys().cloned(),
                    &listed_ids,
                ) {
                    if actively_leased.contains(&shard_id) {
                        continue;
                    }
                    warn!(
                        message = "Deleting leftover checkpoint for shard no longer in ListShards.",
                        stream = %stream.id,
                        shard = %shard_id,
                    );
                    let _ = checkpointer.delete(&stream.id, &shard_id).await;
                }

                for (shard_id, sequence) in &checkpoint_data.sequence_numbers {
                    if !listed_ids.contains(shard_id) || actively_leased.contains(shard_id) {
                        continue;
                    }
                    let children = children_map.get(shard_id).cloned().unwrap_or_default();
                    if should_delete_shard_end_lease(
                        sequence,
                        &children,
                        &checkpoint_data.sequence_numbers,
                    ) {
                        debug!(
                            message = "Deleting SHARD_END parent lease after children started.",
                            stream = %stream.id,
                            shard = %shard_id,
                        );
                        let _ = checkpointer.delete(&stream.id, &shard_id).await;
                    }
                }

                for (client_id, claims) in &checkpoint_data.client_claims {
                    for claim in claims {
                        let elapsed = Utc::now()
                            .signed_duration_since(claim.lease_timeout)
                            .to_std()
                            .unwrap_or(Duration::ZERO);

                        let parents = parent_ids_by_shard
                            .get(&claim.shard_id)
                            .cloned()
                            .unwrap_or_default();
                        let sequence = checkpoint_data.sequence_numbers.get(&claim.shard_id);
                        let closed = closed_by_id.get(&claim.shard_id).copied().unwrap_or(false);
                        let eligible = should_claim_shard(
                            closed,
                            true,
                            sequence.map(String::as_str),
                            &parents,
                            &checkpoint_data.sequence_numbers,
                        );

                        if elapsed > lease_period * 2 && eligible {
                            unclaimed.insert(claim.shard_id.clone(), client_id.clone());
                        } else {
                            unclaimed.remove(&claim.shard_id);
                        }
                    }
                }

                if !unclaimed.is_empty() {
                    for (shard_id, from_client) in &unclaimed {
                        match checkpointer.claim(&stream.id, shard_id, from_client).await {
                            Ok(seq) => {
                                if is_shard_end(&seq) {
                                    let _ = checkpointer
                                        .checkpoint(&stream.id, shard_id, SHARD_END, true)
                                        .await;
                                    continue;
                                }
                                let src = Arc::clone(&self);
                                let chk = Arc::clone(&checkpointer);
                                let out2 = out.clone();
                                let cancel2 = cancel.clone();
                                let stream2 = stream.clone();
                                let shard_id2 = shard_id.clone();
                                let shard_closed =
                                    closed_by_id.get(shard_id).copied().unwrap_or(false);
                                let is_child = is_child_shard(
                                    parent_ids_by_shard
                                        .get(shard_id)
                                        .map(Vec::as_slice)
                                        .unwrap_or(&[]),
                                );
                                task_set.spawn(
                                    async move {
                                        src.run_shard_consumer(
                                            stream2,
                                            shard_id2,
                                            seq,
                                            chk,
                                            out2,
                                            cancel2,
                                            shard_closed,
                                            is_child,
                                        )
                                        .await;
                                    }
                                    .in_current_span(),
                                );
                            }
                            Err(CheckpointerError::LeaseNotAcquired) => {}
                            Err(e) => {
                                if cancel.is_cancelled() {
                                    break 'outer;
                                }
                                warn!(message = "Failed to claim shard.", shard = %shard_id, error = %e);
                            }
                        }
                    }
                } else {
                    // Consider stealing a shard from an overloaded client.
                    let self_claims = checkpoint_data
                        .client_claims
                        .get(&self.client_id)
                        .map(|c| c.len())
                        .unwrap_or(0);

                    'steal: for (client_id, claims) in &checkpoint_data.client_claims {
                        if *client_id == self.client_id {
                            continue;
                        }
                        if claims.len() > self_claims + 1 {
                            let idx = simple_rand() % claims.len();
                            let to_steal = &claims[idx];
                            let parents = parent_ids_by_shard
                                .get(&to_steal.shard_id)
                                .cloned()
                                .unwrap_or_default();
                            let sequence = checkpoint_data.sequence_numbers.get(&to_steal.shard_id);
                            let closed = closed_by_id
                                .get(&to_steal.shard_id)
                                .copied()
                                .unwrap_or(false);
                            if !should_claim_shard(
                                closed,
                                true,
                                sequence.map(String::as_str),
                                &parents,
                                &checkpoint_data.sequence_numbers,
                            ) {
                                continue;
                            }
                            match checkpointer
                                .claim(&stream.id, &to_steal.shard_id, client_id)
                                .await
                            {
                                Ok(seq) => {
                                    if is_shard_end(&seq) {
                                        let _ = checkpointer
                                            .checkpoint(
                                                &stream.id,
                                                &to_steal.shard_id,
                                                SHARD_END,
                                                true,
                                            )
                                            .await;
                                        break 'steal;
                                    }
                                    let src = Arc::clone(&self);
                                    let chk = Arc::clone(&checkpointer);
                                    let out2 = out.clone();
                                    let cancel2 = cancel.clone();
                                    let stream2 = stream.clone();
                                    let shard_id2 = to_steal.shard_id.clone();
                                    let shard_closed = closed;
                                    let is_child = is_child_shard(&parents);
                                    task_set.spawn(
                                        async move {
                                            src.run_shard_consumer(
                                                stream2,
                                                shard_id2,
                                                seq,
                                                chk,
                                                out2,
                                                cancel2,
                                                shard_closed,
                                                is_child,
                                            )
                                            .await;
                                        }
                                        .in_current_span(),
                                    );
                                    break 'steal;
                                }
                                Err(CheckpointerError::LeaseNotAcquired) => {}
                                Err(e) => {
                                    warn!(message = "Failed to steal shard.", shard = %to_steal.shard_id, error = %e);
                                }
                            }
                            break 'steal;
                        }
                    }
                }

                if cancel.is_cancelled() {
                    break 'outer;
                }
            }

            // Reap completed tasks.
            while task_set.try_join_next().is_some() {}

            if cancel.is_cancelled() {
                break;
            }

            let jitter = Duration::from_millis(simple_rand() as u64 % 5_000);
            select! {
                _ = sleep(rebalance_interval + jitter) => {}
                _ = cancel.cancelled() => { break; }
            }
        }

        while task_set.join_next().await.is_some() {}
    }

    // -------------------------------------------------------------------------
    // Explicit mode
    // -------------------------------------------------------------------------

    async fn run_explicit(
        self: Arc<Self>,
        streams: Vec<StreamEntry>,
        checkpointer: Arc<KinesisCheckpointer>,
        out: SourceSender,
        cancel: CancellationToken,
    ) {
        let mut task_set: JoinSet<()> = JoinSet::new();
        let mut pending: Vec<(StreamEntry, String)> = Vec::new();

        for stream in streams {
            for shard_id in &stream.explicit_shards {
                pending.push((stream.clone(), shard_id.clone()));
            }
        }

        while !pending.is_empty() && !cancel.is_cancelled() {
            let mut still_pending = Vec::new();
            for (stream, shard_id) in pending.drain(..) {
                match checkpointer.claim(&stream.id, &shard_id, "").await {
                    Ok(seq) => {
                        let src = Arc::clone(&self);
                        let chk = Arc::clone(&checkpointer);
                        let out2 = out.clone();
                        let cancel2 = cancel.clone();
                        task_set.spawn(
                            async move {
                                src.run_shard_consumer(
                                    stream, shard_id, seq, chk, out2, cancel2, false, false,
                                )
                                .await;
                            }
                            .in_current_span(),
                        );
                    }
                    Err(e) => {
                        if cancel.is_cancelled() {
                            break;
                        }
                        error!(message = "Failed to start shard consumer, will retry.", shard = %shard_id, error = %e);
                        still_pending.push((stream, shard_id));
                    }
                }
            }
            pending = still_pending;

            if !pending.is_empty() {
                select! {
                    _ = sleep(Duration::from_secs(1)) => {}
                    _ = cancel.cancelled() => { break; }
                }
            }
        }

        while task_set.join_next().await.is_some() {}
    }

    // -------------------------------------------------------------------------
    // Per-shard consumer
    // -------------------------------------------------------------------------

    async fn run_shard_consumer(
        self: Arc<Self>,
        stream: StreamEntry,
        shard_id: String,
        starting_sequence: String,
        checkpointer: Arc<KinesisCheckpointer>,
        mut out: SourceSender,
        cancel: CancellationToken,
        mut shard_closed: bool,
        is_child: bool,
    ) {
        debug!(
            message = "Starting shard consumer.",
            stream = %stream.id,
            shard = %shard_id,
            shard_closed,
            is_child,
            client_id = %self.client_id,
        );

        // Stagger startup across shard consumers to avoid synchronized GetRecords bursts.
        let startup_jitter = Duration::from_millis(simple_rand() as u64 % 500);
        select! {
            _ = sleep(startup_jitter) => {}
            _ = cancel.cancelled() => { return; }
        }

        // Clamp the user-configured poll interval to the [200, 1000] ms range.
        // 200ms is the floor imposed by the AWS hard limit of 5 GetRecords/sec/shard.
        // 1000ms is the ceiling, matching the KCL idleTimeBetweenReadsInMillis default.
        let poll_floor_ms = self
            .config
            .poll_interval_ms
            .clamp(GET_RECORDS_MIN_INTERVAL_MS, POLL_INTERVAL_MAX_MS);

        let commit_period =
            Duration::from_secs(self.config.dynamodb.commit_period_secs.clamp(1, 1000));

        let tracker = Arc::new(SequenceTracker::new(
            self.config.checkpoint_limit,
            starting_sequence.clone(),
        ));

        let events_received = register!(EventsReceived);

        if is_shard_end(&starting_sequence) {
            debug!(
                message = "Shard already at SHARD_END; releasing lease without polling.",
                shard = %shard_id,
            );
            let _ = checkpointer
                .checkpoint(&stream.id, &shard_id, SHARD_END, true)
                .await;
            return;
        }

        let mut iter = match self
            .get_shard_iterator(
                &stream.arn,
                &shard_id,
                &starting_sequence,
                self.config.start_from_oldest,
                shard_closed,
                is_child,
            )
            .await
        {
            Ok(it) => it,
            Err(e) => {
                error!(message = "Failed to get shard iterator.", shard = %shard_id, error = %e);
                match iterator_error_action(shard_closed, e.is_resource_not_found()) {
                    IteratorErrorAction::Delete => {
                        warn!(
                            message = "Deleting checkpoint after GetShardIterator failure on missing shard.",
                            shard = %shard_id,
                        );
                        let _ = checkpointer.delete(&stream.id, &shard_id).await;
                    }
                    IteratorErrorAction::ShardEnd => {
                        warn!(
                            message = "Checkpointing SHARD_END after GetShardIterator failure on closed shard.",
                            shard = %shard_id,
                        );
                        let _ = checkpointer
                            .checkpoint(&stream.id, &shard_id, SHARD_END, true)
                            .await;
                    }
                    IteratorErrorAction::Release => {
                        let _ = checkpointer
                            .checkpoint(&stream.id, &shard_id, &starting_sequence, true)
                            .await;
                    }
                }
                return;
            }
        };

        let mut backoff_ms = poll_floor_ms;
        let mut throttle_backoff_ms = THROTTLE_BACKOFF_INITIAL_MS;
        let mut last_commit = tokio::time::Instant::now();
        // Delay to wait before the next GetRecords call.  Updated adaptively based on
        // response size (throughput budget) or error type.  Starts at poll_floor_ms
        // (clamped to [200, 1000] ms) to respect the configured polling interval.
        let mut next_call_delay = Duration::from_millis(poll_floor_ms);
        let mut shard_finished = false;
        let mut shard_gone = false;
        let mut still_owned = true;
        let mut last_millis_behind: Option<i64> = None;
        let mut growing_empty_polls: u32 = 0;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            // Periodic checkpoint.
            if last_commit.elapsed() >= commit_period {
                let seq = tracker.acked_sequence();
                match checkpointer
                    .checkpoint(&stream.id, &shard_id, &seq, false)
                    .await
                {
                    Ok(owned) => {
                        still_owned = owned;
                        if !still_owned {
                            debug!(message = "Shard ownership lost; yielding.", shard = %shard_id);
                            let _ = checkpointer.yield_shard(&stream.id, &shard_id, &seq).await;
                            break;
                        }
                    }
                    Err(e) => {
                        error!(message = "Failed to checkpoint shard.", shard = %shard_id, error = %e);
                    }
                }
                last_commit = tokio::time::Instant::now();
            }

            // Back-pressure: wait until there is room for a full batch before
            // issuing the next GetRecords call.  Checking for max_records_per_call
            // rather than 1 prevents the in-flight count from significantly
            // exceeding checkpoint_limit when large batches are returned.
            if !tracker.can_accept(self.config.max_records_per_call as i64) {
                select! {
                    _ = sleep(Duration::from_millis(10)) => { continue; }
                    _ = cancel.cancelled() => { break; }
                }
            }

            // Adaptive pacing: wait based on throughput budget consumed by the previous
            // response.  Enforces the 5 calls/sec hard limit (min 200ms) and additionally
            // backs off proportionally when a response consumed a large fraction of the
            // 2 MiB/sec per-shard sliding budget.
            select! {
                _ = sleep(next_call_delay) => {}
                _ = cancel.cancelled() => { break; }
            }

            let get_result = self
                .kinesis
                .get_records()
                .stream_arn(&stream.arn)
                .shard_iterator(&iter)
                .limit(self.config.max_records_per_call.min(KINESIS_MAX_RECORDS))
                .send()
                .await;

            match get_result {
                Err(e) => {
                    let is_throughput_exceeded = e
                        .as_service_error()
                        .map(|se| se.is_provisioned_throughput_exceeded_exception())
                        .unwrap_or(false);
                    let is_expired = e
                        .as_service_error()
                        .map(|se| se.is_expired_iterator_exception())
                        .unwrap_or(false);

                    let is_not_found = e
                        .as_service_error()
                        .map(|se| se.is_resource_not_found_exception())
                        .unwrap_or(false);

                    if is_throughput_exceeded {
                        // Use a dedicated backoff starting at 1s (AWS recommendation).
                        // The sleep happens at the top of the next loop iteration.
                        let jitter_ms = simple_rand() as u64 % (throttle_backoff_ms / 2).max(1);
                        let delay_ms = throttle_backoff_ms + jitter_ms;
                        warn!(
                            message = "Kinesis read throughput exceeded; backing off.",
                            shard = %shard_id,
                            delay_ms = delay_ms,
                        );
                        next_call_delay = Duration::from_millis(delay_ms);
                        throttle_backoff_ms =
                            (throttle_backoff_ms * 2).min(THROTTLE_BACKOFF_MAX_MS);
                        continue;
                    } else if is_expired {
                        warn!(message = "Shard iterator expired, refreshing.", shard = %shard_id);
                        let seq = tracker.acked_sequence();
                        match self
                            .get_shard_iterator(
                                &stream.arn,
                                &shard_id,
                                &seq,
                                self.config.start_from_oldest,
                                shard_closed,
                                is_child,
                            )
                            .await
                        {
                            Ok(new_iter) => {
                                iter = new_iter;
                                next_call_delay = Duration::from_millis(poll_floor_ms);
                                continue;
                            }
                            Err(re) => {
                                error!(message = "Failed to refresh shard iterator.", error = %re);
                                match iterator_error_action(
                                    shard_closed,
                                    re.is_resource_not_found(),
                                ) {
                                    IteratorErrorAction::Delete => {
                                        shard_finished = true;
                                        shard_gone = true;
                                        break;
                                    }
                                    IteratorErrorAction::ShardEnd => {
                                        shard_finished = true;
                                        break;
                                    }
                                    IteratorErrorAction::Release => {}
                                }
                            }
                        }
                    } else if is_not_found {
                        warn!(
                            message = "Shard no longer exists; completing consumer.",
                            shard = %shard_id,
                        );
                        shard_finished = true;
                        shard_gone = true;
                        break;
                    } else if !cancel.is_cancelled() {
                        error!(
                            message = "GetRecords error.",
                            shard = %shard_id,
                            error = %DisplayErrorContext(&e),
                        );
                    }

                    let jitter_ms = simple_rand() as u64 % (backoff_ms / 2).max(1);
                    next_call_delay = Duration::from_millis(backoff_ms + jitter_ms);
                    backoff_ms = (backoff_ms * 2).min(BACKOFF_MAX_MS);
                    continue;
                }
                Ok(output) => {
                    if !output.child_shards().is_empty() {
                        shard_closed = true;
                    }

                    let next_iterator_missing = match output.next_shard_iterator() {
                        Some(next) if !next.is_empty() => {
                            iter = next.to_string();
                            false
                        }
                        _ => true,
                    };

                    let millis_behind = output.millis_behind_latest();
                    let records = output.records;
                    let records_empty = records.is_empty();
                    growing_empty_polls = next_growing_empty_polls(
                        records_empty,
                        last_millis_behind,
                        millis_behind,
                        growing_empty_polls,
                    );
                    last_millis_behind = millis_behind;

                    if should_finish_shard_consumer(
                        next_iterator_missing,
                        records_empty,
                        shard_closed,
                        growing_empty_polls,
                    ) {
                        if shard_closed && !next_iterator_missing {
                            debug!(
                                message = "Closed shard drained; completing despite next iterator.",
                                shard = %shard_id,
                                growing_empty_polls,
                            );
                        }
                        shard_finished = true;
                    }

                    if records_empty {
                        // Shard is caught up; back off before the next poll.
                        // The sleep happens at the top of the next loop iteration.
                        let jitter_ms = simple_rand() as u64 % (backoff_ms / 2).max(1);
                        next_call_delay = Duration::from_millis(backoff_ms + jitter_ms);
                        backoff_ms = (backoff_ms * 2).min(BACKOFF_MAX_MS);
                        if shard_finished {
                            break;
                        }
                        continue;
                    }

                    // Successful read with data: reset error backoffs and compute
                    // throughput-aware pacing.  A response of N bytes consumes N/2MiB
                    // seconds of the per-shard read budget, so wait at least that long
                    // before issuing the next call (floored at poll_floor_ms).
                    backoff_ms = poll_floor_ms;
                    throttle_backoff_ms = THROTTLE_BACKOFF_INITIAL_MS;
                    let response_bytes: u64 =
                        records.iter().map(|r| r.data().as_ref().len() as u64).sum();
                    let budget_ms = (response_bytes as f64 / SHARD_READ_BUDGET_BYTES_PER_SEC as f64
                        * 1000.0) as u64;
                    next_call_delay = Duration::from_millis(budget_ms.max(poll_floor_ms));

                    let record_count = records.len() as i64;

                    // Decode each record into events, isolating failures per-record so
                    // that a single malformed record (poison pill) cannot stall or kill
                    // the shard consumer.
                    let mut all_events: Vec<Event> = Vec::with_capacity(records.len());
                    let mut last_sequence = String::new();

                    for record in &records {
                        let seq_num = record.sequence_number().to_string();
                        let record_events = self.decode_record(record, &stream.id, &shard_id);
                        for event in &record_events {
                            events_received
                                .emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
                        }
                        all_events.extend(record_events);
                        last_sequence = seq_num;
                    }

                    if all_events.is_empty() {
                        // Records decoded to nothing. Nothing was tracked, so do not
                        // call acknowledge (which would decrement in_flight below zero).
                        // Only advance the acked sequence so the next checkpoint reflects
                        // that these records were consumed.
                        tracker.advance_sequence(last_sequence);
                        if shard_finished {
                            break;
                        }
                        continue;
                    }

                    tracker.track(record_count);

                    // Wire up acknowledgements.
                    let (batch, batch_receiver) =
                        BatchNotifier::maybe_new_with_receiver(self.acknowledgements);

                    let events_with_batch: Vec<Event> = all_events
                        .into_iter()
                        .map(|e| e.with_batch_notifier_option(&batch))
                        .collect();

                    drop(batch);

                    // Spawn ack handler.
                    let tracker2 = Arc::clone(&tracker);
                    let ack_seq = last_sequence.clone();
                    let ack_count = record_count;
                    if let Some(receiver) = batch_receiver {
                        tokio::spawn(async move {
                            let status = receiver.await;
                            if status == BatchStatus::Delivered {
                                tracker2.acknowledge(ack_count, ack_seq);
                            } else {
                                tracker2.release(ack_count);
                            }
                        });
                    } else {
                        // No acknowledgement mode: mark delivered immediately.
                        tracker.acknowledge(record_count, last_sequence.clone());
                    }

                    if out.send_batch(events_with_batch).await.is_err() {
                        debug!(message = "Output channel closed, stopping.", shard = %shard_id);
                        break;
                    }

                    if shard_finished {
                        break;
                    }
                }
            }
        }

        // Final cleanup. Fully consumed shards get KCL SHARD_END so children can
        // start; shards that no longer exist have their leftover row deleted.
        let final_seq = tracker.acked_sequence();
        if shard_finished && still_owned {
            if shard_gone {
                debug!(
                    message = "Shard gone; deleting leftover checkpoint.",
                    shard = %shard_id,
                );
                let _ = checkpointer.delete(&stream.id, &shard_id).await;
            } else {
                debug!(
                    message = "Shard fully consumed; checkpointing SHARD_END.",
                    shard = %shard_id,
                );
                let _ = checkpointer
                    .checkpoint(&stream.id, &shard_id, SHARD_END, true)
                    .await;
            }
        } else if still_owned {
            let _ = checkpointer
                .checkpoint(&stream.id, &shard_id, &final_seq, true)
                .await;
        }

        debug!(
            message = "Shard consumer finished.",
            stream = %stream.id,
            shard = %shard_id,
        );
    }

    // -------------------------------------------------------------------------
    // Per-record decoding
    // -------------------------------------------------------------------------

    /// Decode a single Kinesis record into events, inserting stream/shard metadata.
    ///
    /// Failures are isolated here: a record that cannot be decoded produces zero
    /// events and logs a warning rather than propagating an error to the caller.
    /// This is the equivalent of Logstash's `rescue => error` around `process_record`,
    /// ensuring that one malformed record cannot kill the entire shard consumer.
    fn decode_record(&self, record: &Record, stream_id: &str, shard_id: &str) -> Vec<Event> {
        let data = record.data().as_ref().to_vec();
        let timestamp = record.approximate_arrival_timestamp().and_then(|ts| {
            let secs = ts.secs();
            let nanos = ts.subsec_nanos();
            Utc.timestamp_opt(secs, nanos).single()
        });

        let schema = log_schema();
        let partition_key = record.partition_key().to_string();
        let seq_num = record.sequence_number().to_string();

        let mut buf = bytes::BytesMut::from(data.as_slice());
        let mut decoder = self.decoder.clone();
        let mut events: Vec<Event> = Vec::new();

        loop {
            match decoder.decode_eof(&mut buf) {
                Ok(Some((decoded, _))) => {
                    for mut event in decoded {
                        if let Event::Log(ref mut log) = event {
                            match self.log_namespace {
                                LogNamespace::Vector => {
                                    if let Some(ts) = timestamp {
                                        log.try_insert(
                                            metadata_path!("aws_kinesis_streams", "timestamp"),
                                            ts,
                                        );
                                    }
                                    log.insert(
                                        metadata_path!("vector", "ingest_timestamp"),
                                        Utc::now(),
                                    );
                                    log.try_insert(
                                        metadata_path!("aws_kinesis_streams", "kinesis_stream"),
                                        stream_id.to_string(),
                                    );
                                    log.try_insert(
                                        metadata_path!("aws_kinesis_streams", "kinesis_shard"),
                                        shard_id.to_string(),
                                    );
                                    log.try_insert(
                                        metadata_path!(
                                            "aws_kinesis_streams",
                                            "kinesis_partition_key"
                                        ),
                                        partition_key.clone(),
                                    );
                                    log.try_insert(
                                        metadata_path!(
                                            "aws_kinesis_streams",
                                            "kinesis_sequence_number"
                                        ),
                                        seq_num.clone(),
                                    );
                                }
                                LogNamespace::Legacy => {
                                    if let Some(ts) = timestamp {
                                        if let Some(timestamp_key) = schema.timestamp_key() {
                                            log.try_insert((PathPrefix::Event, timestamp_key), ts);
                                        }
                                    }
                                    log.try_insert("kinesis_stream", stream_id.to_string());
                                    log.try_insert("kinesis_shard", shard_id.to_string());
                                    log.try_insert("kinesis_partition_key", partition_key.clone());
                                    log.try_insert("kinesis_sequence_number", seq_num.clone());
                                }
                            }
                        }
                        events.push(event);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    if !e.can_continue() {
                        warn!(
                            message = "Failed to decode Kinesis record; skipping.",
                            shard = %shard_id,
                            sequence = %seq_num,
                            error = %e,
                        );
                        break;
                    }
                }
            }
        }

        events
    }
}

/// Error obtaining a Kinesis shard iterator.
#[derive(Debug)]
enum ShardIteratorError {
    ResourceNotFound(String),
    Other(String),
}

impl ShardIteratorError {
    fn new(resource_not_found: bool, fallback: bool, ctx: impl std::fmt::Display) -> Self {
        let prefix = if fallback {
            "GetShardIterator fallback error"
        } else {
            "GetShardIterator error"
        };
        let msg = format!("{prefix}: {ctx}");
        if resource_not_found {
            Self::ResourceNotFound(msg)
        } else {
            Self::Other(msg)
        }
    }

    fn is_resource_not_found(&self) -> bool {
        matches!(self, Self::ResourceNotFound(_))
    }
}

impl std::fmt::Display for ShardIteratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceNotFound(m) | Self::Other(m) => f.write_str(m),
        }
    }
}

/// Deterministic pseudo-random index based on nanoseconds, used for shard stealing.
fn simple_rand() -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
}

/// Split `"stream-name:shard-id"` into `("stream-name", "shard-id")`.
/// Handles ARNs like `"arn:aws:kinesis:us-east-1:123:stream/my-stream:0"`.
pub(crate) fn parse_stream_id(id: &str) -> crate::Result<(String, String)> {
    let (prefix, tail) = if let Some(slash) = id.rfind('/') {
        (&id[..slash + 1], &id[slash + 1..])
    } else {
        ("", id)
    };

    let parts: Vec<&str> = tail.splitn(3, ':').collect();
    match parts.len() {
        1 => Ok((format!("{prefix}{}", parts[0].trim()), String::new())),
        2 => Ok((
            format!("{prefix}{}", parts[0].trim()),
            parts[1].trim().to_string(),
        )),
        _ => Err(format!(
            "Stream '{}' is invalid: only one shard may be specified per entry.",
            id
        )
        .into()),
    }
}
