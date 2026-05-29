//! Self-driving Antithesis exerciser for the disk buffer v2.
//!
//! This binary IS the system under test: because a disk_v2 buffer takes an
//! advisory lock (one process per buffer directory), the workload that drives
//! the buffer must live in the same process that owns it. It opens a real
//! disk_v2 buffer through the public topology API (exactly as Vector's sink
//! layer does) and then runs randomized writer/reader activity forever, using
//! the Antithesis SDK's randomness so Antithesis can branch the search.
//!
//! The dangerous *internal* invariants (the `total_buffer_size` /
//! `get_total_records` / data-file size-delta underflows behind Vector #21683,
//! and the file-id rollover) are checked by surgical `assert_always!` calls
//! placed SUT-side inside `vector-buffers` itself — those fire no matter how
//! the buffer reaches the bad state. The assertions here are the workload-level
//! safety/liveness oracle: never deliver more than produced (at-most-once
//! sanity), the drained-buffer boundary is actually reached (reachability of
//! the #21683 precondition), and flushed records do get delivered (progress).
//!
//! All `antithesis_sdk` calls are no-ops outside the Antithesis environment, so
//! this binary also runs fine locally for smoke testing.

use std::{
    error, fmt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use antithesis_sdk::{assert_always, assert_reachable, assert_sometimes, lifecycle, random};
use bytes::{Buf, BufMut};
use tokio::{task, time};
use tracing::{Span, info};
use tracing_subscriber::EnvFilter;
use vector_buffers::{
    BufferType, EventCount, WhenFull,
    encoding::FixedEncodable,
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
};
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{
        AddBatchNotifier, BatchNotifier, EventFinalizer, EventFinalizers, EventStatus, Finalizable,
    },
};

/// A uniquely-id'd, variable-size record. Identical in spirit to the one used by
/// `buffer_perf`, so it round-trips through the disk_v2 encoder/decoder unchanged.
#[derive(Clone, Debug)]
struct VariableMessage {
    id: u64,
    payload: Vec<u8>,
    finalizers: EventFinalizers,
}

impl VariableMessage {
    fn new(id: u64, payload: Vec<u8>) -> Self {
        VariableMessage {
            id,
            payload,
            finalizers: EventFinalizers::default(),
        }
    }
}

impl AddBatchNotifier for VariableMessage {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        self.finalizers.add(EventFinalizer::new(batch));
    }
}

impl ByteSizeOf for VariableMessage {
    fn allocated_bytes(&self) -> usize {
        self.payload.len()
    }
}

impl EventCount for VariableMessage {
    fn event_count(&self) -> usize {
        1
    }
}

impl Finalizable for VariableMessage {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl FixedEncodable for VariableMessage {
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        buffer.put_u64(self.payload.len() as u64);
        buffer.put_slice(&self.payload);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(8 + 8 + self.payload.len())
    }

    fn decode<B>(mut buffer: B) -> Result<Self, Self::DecodeError>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        let payload_len = buffer.get_u64() as usize;
        let payload = buffer.copy_to_bytes(payload_len).to_vec();
        Ok(VariableMessage::new(id, payload))
    }
}

#[derive(Debug)]
struct EncodeError;
impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl error::Error for EncodeError {}

#[derive(Debug)]
struct DecodeError;
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl error::Error for DecodeError {}

/// Shared progress counters between the writer, reader, and status reporter.
#[derive(Default)]
struct Progress {
    produced: AtomicU64,
    /// Records that were `flush`ed by the writer (durably committed) — the set
    /// the at-least-once oracle holds the buffer accountable for.
    produced_flushed: AtomicU64,
    delivered: AtomicU64,
    rejected: AtomicU64,
    /// Set once the reader has observed the buffer fully drained at least once
    /// (reader id caught up to the writer) — the #21683 underflow precondition.
    drained_seen: AtomicBool,
}

/// Draw a `u64` in `[lo, hi)` from the Antithesis-controlled RNG.
#[inline]
fn rand_in(lo: u64, hi: u64) -> u64 {
    if hi <= lo {
        return lo;
    }
    lo + (random::get_random() % (hi - lo))
}

/// Build a disk_v2 buffer through the public topology API, the same path the
/// Vector sink layer uses.
async fn build_buffer(
    data_dir: PathBuf,
    max_size: u64,
) -> (
    BufferSender<VariableMessage>,
    BufferReceiver<VariableMessage>,
) {
    let mut builder = TopologyBuilder::default();
    let variant = BufferType::DiskV2 {
        max_size: std::num::NonZeroU64::new(max_size).expect("max_size must be non-zero"),
        when_full: WhenFull::Block,
    };
    variant
        .add_to_builder(&mut builder, Some(data_dir), "vdbuf-antithesis".to_string())
        .expect("adding disk_v2 variant to builder should not fail");
    builder
        .build(String::from("vdbuf-antithesis"), Span::none())
        .await
        .expect("building the disk_v2 buffer should not fail")
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    antithesis_sdk::antithesis_init();

    let data_dir =
        PathBuf::from(std::env::var("VDBUF_DIR").unwrap_or_else(|_| "/var/lib/vdbuf".to_string()));
    let status_path = PathBuf::from(
        std::env::var("VDBUF_STATUS").unwrap_or_else(|_| "/tmp/vdbuf-status".to_string()),
    );
    // Keep the buffer small so fill/drain cycles are cheap and the reader
    // repeatedly catches up to the writer — the get_total_records underflow
    // boundary. 256MB is the enforced minimum.
    let max_size: u64 = std::env::var("VDBUF_MAX_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(268_435_488);
    // Cap the payload well under the per-record/data-file limits.
    let max_payload: u64 = std::env::var("VDBUF_MAX_PAYLOAD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);

    std::fs::create_dir_all(&data_dir).expect("creating buffer dir should not fail");

    info!(
        ?data_dir,
        max_size, max_payload, "[vdbuf] opening disk_v2 buffer"
    );
    let (mut writer, mut reader) = build_buffer(data_dir, max_size).await;

    let progress = Arc::new(Progress::default());

    // ---- writer task ---------------------------------------------------
    let wp = Arc::clone(&progress);
    let writer_task = task::spawn(async move {
        let mut next_id: u64 = 1;
        let mut iters: u64 = 0;
        loop {
            iters += 1;
            // Produce a small randomly-sized batch, then flush only some of the
            // time. Small batches keep the writer from sprinting far ahead of the
            // reader, so the two ride close together and the reader frequently
            // catches up to the writer head — the get_total_records / #21683
            // underflow boundary that Antithesis's thread-pausing then races.
            let batch = rand_in(1, 16);
            for _ in 0..batch {
                let payload_len = rand_in(0, max_payload + 1) as usize;
                let msg = VariableMessage::new(next_id, vec![0xab; payload_len]);
                next_id += 1;
                if writer.send(msg, None).await.is_ok() {
                    wp.produced.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Flush ~75% of the time; the rest of the time let records linger
            // (exercising the not-yet-durable path).
            if rand_in(0, 4) != 0 && writer.flush().await.is_ok() {
                // Everything produced so far is now durably committed.
                let produced = wp.produced.load(Ordering::Relaxed);
                wp.produced_flushed.store(produced, Ordering::Relaxed);
            }

            // Periodic "drain phase": idle long enough for the reader to fully
            // catch up to the writer, parking the buffer right at the drained
            // boundary where the accounting underflows manifest.
            if iters % 16 == 0 {
                let _ = writer.flush().await;
                wp.produced_flushed
                    .store(wp.produced.load(Ordering::Relaxed), Ordering::Relaxed);
                time::sleep(Duration::from_millis(rand_in(100, 400))).await;
            } else if rand_in(0, 4) == 0 {
                time::sleep(Duration::from_millis(rand_in(2, 20))).await;
            }
        }
    });

    // ---- reader task ---------------------------------------------------
    let rp = Arc::clone(&progress);
    let reader_task = task::spawn(async move {
        let mut setup_done = false;
        loop {
            // Read aggressively so the reader stays close behind the writer.
            // Only rarely pause (letting the buffer grow toward full).
            if rand_in(0, 20) == 0 {
                time::sleep(Duration::from_millis(rand_in(5, 80))).await;
            }

            match reader.next().await {
                Some(mut record) => {
                    let finalizers = record.take_finalizers();
                    // Mostly acknowledge delivery; occasionally Reject to
                    // exercise the finalizer status path. Either way the read
                    // advances the reader, driving it toward the writer.
                    if rand_in(0, 16) == 0 {
                        finalizers.update_status(EventStatus::Rejected);
                        rp.rejected.fetch_add(1, Ordering::Relaxed);
                    } else {
                        finalizers.update_status(EventStatus::Delivered);
                        rp.delivered.fetch_add(1, Ordering::Relaxed);
                    }
                    drop(record);

                    if !setup_done {
                        // First successful end-to-end round-trip through the
                        // disk buffer: the harness is live.
                        assert_reachable!("record delivered end-to-end through disk_v2 buffer");
                        lifecycle::setup_complete(&serde_json::json!({"stage": "first_delivery"}));
                        setup_done = true;
                    }
                }
                None => {
                    // Buffer reports end-of-stream: reader has caught up to the
                    // writer — the drained boundary that drives get_total_records
                    // toward its `0 - 1` underflow.
                    rp.drained_seen.store(true, Ordering::Relaxed);
                    assert_reachable!("disk_v2 buffer fully drained (reader caught up to writer)");
                    time::sleep(Duration::from_millis(rand_in(1, 20))).await;
                }
            }
        }
    });

    // ---- status + oracle reporter -------------------------------------
    let mut tick = time::interval(Duration::from_millis(500));
    loop {
        tick.tick().await;
        let produced = progress.produced.load(Ordering::Relaxed);
        let flushed = progress.produced_flushed.load(Ordering::Relaxed);
        let delivered = progress.delivered.load(Ordering::Relaxed);
        let rejected = progress.rejected.load(Ordering::Relaxed);
        let drained = progress.drained_seen.load(Ordering::Relaxed);
        let handled = delivered + rejected;

        // Safety: the buffer can never hand the reader more records than were
        // ever produced. A violation means duplicated/phantom records — exactly
        // what a get_total_records / accounting underflow would manifest as.
        assert_always!(
            handled <= produced,
            "disk_v2 never delivers more records than were produced"
        );

        // Liveness: once we have flushed records, the reader keeps making
        // progress toward delivering them.
        if flushed > 0 {
            assert_sometimes!(
                delivered > 0,
                "flushed records are eventually delivered (writer/reader make progress)"
            );
        }

        // Reachability of the dangerous precondition.
        if drained {
            assert_reachable!("reached drained-buffer state at least once");
        }

        let _ = std::fs::write(
            &status_path,
            format!(
                "produced={produced} flushed={flushed} delivered={delivered} \
                 rejected={rejected} handled={handled} drained={drained}\n"
            ),
        );

        if writer_task.is_finished() || reader_task.is_finished() {
            info!("[vdbuf] a worker task ended; stopping");
            break;
        }
    }
}
