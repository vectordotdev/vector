//! Self-driving Antithesis **data-loss finder** for the disk buffer v2.
//!
//! Where `disk_v2_antithesis` checks the internal accounting invariants
//! (#21683-class underflows), this binary is a phased scenario loop whose sole
//! job is to detect *silent data loss*: a record that the buffer accepted and
//! durably flushed (an at-least-once promise) but then never hands back to the
//! reader and never explicitly refuses.
//!
//! It is a single-process, single-task harness so that record ids are strictly
//! monotonic (1, 2, 3, ...) and we can run a precise oracle:
//!
//! * `outstanding` — ids produced AND flushed but not yet resolved.
//! * `allowed_loss` — ids whose loss is permitted (unflushed at crash/reopen).
//!
//! On each round we pick one of seven scenarios, run an ACTIVE phase that
//! injects the scenario's fault, a QUIESCE phase that drains the reader, and a
//! CHECK phase that asserts `outstanding - allowed_loss` is empty. A non-empty
//! leftover is silent data loss and (per owner ruling) a BUG, so the
//! `assert_always!` calls are *meant* to fire if the buffer loses data.
//!
//! All `antithesis_sdk` calls are no-ops outside the Antithesis environment, so
//! this binary also runs fine locally for smoke testing.

use std::{
    collections::BTreeSet,
    error, fmt,
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write as _},
    path::{Path, PathBuf},
    time::Duration,
};

use antithesis_sdk::{assert_always, assert_reachable, lifecycle, random};
use bytes::{Buf, BufMut};
use tokio::time;
use tracing::{Span, info, warn};
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
/// `buffer_perf` / `disk_v2_antithesis`, so it round-trips through the disk_v2
/// encoder/decoder unchanged.
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

/// Draw a `u64` in `[lo, hi)` from the Antithesis-controlled RNG.
#[inline]
fn rand_in(lo: u64, hi: u64) -> u64 {
    if hi <= lo {
        return lo;
    }
    lo + (random::get_random() % (hi - lo))
}

/// Build a disk_v2 buffer through the public topology API, the same path the
/// Vector sink layer uses. `when_full` and `max_size` are scenario-controlled.
async fn build_buffer(
    data_dir: PathBuf,
    max_size: u64,
    when_full: WhenFull,
) -> (
    BufferSender<VariableMessage>,
    BufferReceiver<VariableMessage>,
) {
    let mut builder = TopologyBuilder::default();
    let variant = BufferType::DiskV2 {
        max_size: std::num::NonZeroU64::new(max_size).expect("max_size must be non-zero"),
        when_full,
    };
    variant
        .add_to_builder(&mut builder, Some(data_dir), "vdbuf-lossfinder".to_string())
        .expect("adding disk_v2 variant to builder should not fail");
    builder
        .build(String::from("vdbuf-lossfinder"), Span::none())
        .await
        .expect("building the disk_v2 buffer should not fail")
}

/// Recursively find any existing `*.dat` data file under `dir`.
fn find_dat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_dat_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("dat") {
            out.push(path);
        }
    }
}

/// Scenario tags. `scenario = get_random() % 7`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Baseline,
    WriterDropNoFlush,
    RejectDeliveries,
    CrashReopen,
    DropNewestOverfill,
    Corruption,
    TruncateTail,
}

impl Scenario {
    fn from_u64(n: u64) -> Self {
        match n % 7 {
            0 => Scenario::Baseline,
            1 => Scenario::WriterDropNoFlush,
            2 => Scenario::RejectDeliveries,
            3 => Scenario::CrashReopen,
            4 => Scenario::DropNewestOverfill,
            5 => Scenario::Corruption,
            _ => Scenario::TruncateTail,
        }
    }
}

/// Running oracle / counters shared across rounds.
#[derive(Default)]
struct Oracle {
    /// ids produced AND flushed but not yet resolved (delivered/dropped).
    outstanding: BTreeSet<u64>,
    /// ids whose loss is permitted (unflushed at writer-drop / crash).
    allowed_loss: BTreeSet<u64>,
    produced: u64,
    produced_flushed: u64,
    delivered: u64,
    rejected: u64,
    /// ids the buffer refused at send-time (drop_newest accounting).
    dropped_counted: u64,
    silent_loss_detected: u64,
    /// Largest id ever delivered, for the monotonic sanity assertion.
    max_delivered_id: u64,
}

/// State for a single buffer instance (sender/receiver + its config).
struct BufInstance {
    writer: BufferSender<VariableMessage>,
    reader: BufferReceiver<VariableMessage>,
}

/// Read a handful of records and ack them. In `RejectDeliveries`, ack a fraction
/// as Rejected and intentionally KEEP them in `outstanding` — a rejected event
/// must be retained/redelivered, not silently freed. Returns whether a delivery
/// happened (for first-delivery lifecycle signalling).
async fn read_some(
    buf: &mut BufInstance,
    oracle: &mut Oracle,
    count: u64,
    reject_fraction: bool,
) -> bool {
    let mut any_delivered = false;
    for _ in 0..count {
        match buf.reader.next().await {
            Some(mut record) => {
                let id = record.id;
                let finalizers = record.take_finalizers();
                if reject_fraction && rand_in(0, 3) == 0 {
                    // Rejected: the buffer must NOT consider this resolved.
                    finalizers.update_status(EventStatus::Rejected);
                    oracle.rejected += 1;
                    // Deliberately do NOT remove `id` from `outstanding`.
                } else {
                    finalizers.update_status(EventStatus::Delivered);
                    oracle.delivered += 1;
                    oracle.max_delivered_id = oracle.max_delivered_id.max(id);
                    oracle.outstanding.remove(&id);
                    any_delivered = true;
                }
                drop(record);
            }
            None => break,
        }
    }
    any_delivered
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    antithesis_sdk::antithesis_init();

    let base_dir =
        PathBuf::from(std::env::var("VDBUF_DIR").unwrap_or_else(|_| "/var/lib/vdbuf".to_string()));
    let status_path = PathBuf::from(
        std::env::var("VDBUF_STATUS").unwrap_or_else(|_| "/tmp/vdbuf-status".to_string()),
    );
    let max_size: u64 = std::env::var("VDBUF_MAX_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(268_435_488);
    let max_payload: u64 = std::env::var("VDBUF_MAX_PAYLOAD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);

    std::fs::create_dir_all(&base_dir).expect("creating base buffer dir should not fail");

    let mut oracle = Oracle::default();
    let mut next_id: u64 = 1;
    let mut setup_done = false;
    let mut round: u64 = 0;
    // Rotate the on-disk subdir periodically to avoid unbounded disk growth.
    let mut subdir_seq: u64 = 0;

    let mut data_dir = base_dir.join(format!("run-{subdir_seq}"));
    std::fs::create_dir_all(&data_dir).expect("creating buffer subdir should not fail");
    info!(
        ?data_dir,
        max_size, max_payload, "[lossfinder] opening disk_v2 buffer"
    );
    let (writer, reader) = build_buffer(data_dir.clone(), max_size, WhenFull::Block).await;
    let mut buf = BufInstance { writer, reader };

    loop {
        round += 1;
        let scenario = Scenario::from_u64(random::get_random());

        // ----- scenario coverage markers ----------------------------------
        match scenario {
            Scenario::Baseline => assert_reachable!("scenario: Baseline entered"),
            Scenario::WriterDropNoFlush => {
                assert_reachable!("scenario: WriterDropNoFlush entered")
            }
            Scenario::RejectDeliveries => {
                assert_reachable!("scenario: RejectDeliveries entered")
            }
            Scenario::CrashReopen => assert_reachable!("scenario: CrashReopen entered"),
            Scenario::DropNewestOverfill => {
                assert_reachable!("scenario: DropNewestOverfill entered")
            }
            Scenario::Corruption => assert_reachable!("scenario: Corruption entered"),
            Scenario::TruncateTail => assert_reachable!("scenario: TruncateTail entered"),
        }

        // DropNewestOverfill needs its own buffer built with DropNewest and a
        // smaller max_size; rebuild on a fresh subdir for this round only.
        if scenario == Scenario::DropNewestOverfill {
            subdir_seq += 1;
            let dn_dir = base_dir.join(format!("run-{subdir_seq}"));
            std::fs::create_dir_all(&dn_dir).expect("creating drop_newest subdir should not fail");
            drop(buf);
            // Minimum enforced size is 256MB; use it as the "smaller" size.
            let (w, r) = build_buffer(dn_dir.clone(), 268_435_488, WhenFull::DropNewest).await;
            buf = BufInstance {
                writer: w,
                reader: r,
            };
            data_dir = dn_dir;
            // Reset the oracle's outstanding set: drop_newest is best-effort and
            // we do not strict-track its ids (see SIMPLIFICATION below).
            oracle.outstanding.clear();
            oracle.allowed_loss.clear();
        }

        let active_rounds = rand_in(20, 80);
        // Pick a random point in the active phase to inject the fault.
        let fault_at = rand_in(0, active_rounds.max(1));
        // Track ids produced+flushed strictly after a corruption/truncation, to
        // verify the fault doesn't cause collateral loss of *later* records.
        let mut post_fault_ids: BTreeSet<u64> = BTreeSet::new();
        let mut fault_applied = false;
        // For drop_newest we treat everything best-effort and skip strict oracle.
        let drop_newest = scenario == Scenario::DropNewestOverfill;

        // ===================== ACTIVE PHASE ===============================
        for active in 0..active_rounds {
            // Produce a batch of monotonic-id records.
            let batch = rand_in(1, 16);
            let mut newly_produced: Vec<u64> = Vec::new();
            for _ in 0..batch {
                let payload_len = rand_in(0, max_payload + 1) as usize;
                let msg = VariableMessage::new(next_id, vec![0xab; payload_len]);
                let id = next_id;
                next_id += 1;
                match buf.writer.send(msg, None).await {
                    Ok(()) => {
                        oracle.produced += 1;
                        newly_produced.push(id);
                    }
                    Err(e) => {
                        // Under fault injection a send may fail; expected.
                        warn!(error = %e, "[lossfinder] send failed (expected under fault)");
                    }
                }
            }

            // WriterDropNoFlush special-case: when we hit the fault point,
            // produce several records WITHOUT flushing, then drop+reopen the
            // writer. The unflushed ids are allowed loss.
            if scenario == Scenario::WriterDropNoFlush && active == fault_at && !fault_applied {
                fault_applied = true;
                let mut unflushed: Vec<u64> = Vec::new();
                for _ in 0..rand_in(1, 12) {
                    let payload_len = rand_in(0, max_payload + 1) as usize;
                    let msg = VariableMessage::new(next_id, vec![0xab; payload_len]);
                    let id = next_id;
                    next_id += 1;
                    if buf.writer.send(msg, None).await.is_ok() {
                        oracle.produced += 1;
                        unflushed.push(id);
                    }
                }
                for id in &unflushed {
                    oracle.allowed_loss.insert(*id);
                }
                // Also any records produced this iteration but not yet flushed
                // are at risk; conservatively allow their loss too.
                for id in &newly_produced {
                    oracle.allowed_loss.insert(*id);
                }
                newly_produced.clear();
                info!("[lossfinder] WriterDropNoFlush: dropping+reopening writer");
                drop(buf);
                let (w, r) = build_buffer(data_dir.clone(), max_size, WhenFull::Block).await;
                buf = BufInstance {
                    writer: w,
                    reader: r,
                };
                continue;
            }

            // Flush ~75% of the time. On flush, the newly-produced ids become
            // durable and the buffer is accountable for them.
            let do_flush = rand_in(0, 4) != 0;
            if do_flush {
                match buf.writer.flush().await {
                    Ok(()) => {
                        if !drop_newest {
                            for id in &newly_produced {
                                oracle.outstanding.insert(*id);
                                // If a corruption/truncation already happened this
                                // round, these are post-fault and must survive.
                                if fault_applied
                                    && matches!(
                                        scenario,
                                        Scenario::Corruption | Scenario::TruncateTail
                                    )
                                {
                                    post_fault_ids.insert(*id);
                                }
                            }
                        }
                        oracle.produced_flushed += newly_produced.len() as u64;
                    }
                    Err(e) => {
                        warn!(error = %e, "[lossfinder] flush failed (expected under fault)");
                    }
                }
            }

            // Read a few records and ack them.
            let to_read = rand_in(0, 5);
            let reject = scenario == Scenario::RejectDeliveries;
            let delivered_now = read_some(&mut buf, &mut oracle, to_read, reject).await;
            if delivered_now && !setup_done {
                assert_reachable!("first record delivered end-to-end through disk_v2 lossfinder");
                lifecycle::setup_complete(&serde_json::json!({"stage": "first_delivery"}));
                setup_done = true;
            }

            // ----- inject the remaining faults at the fault point ----------
            if active == fault_at && !fault_applied {
                match scenario {
                    Scenario::CrashReopen => {
                        fault_applied = true;
                        // Anything produced this iter but not flushed is at risk.
                        if !do_flush {
                            for id in &newly_produced {
                                oracle.allowed_loss.insert(*id);
                            }
                        }
                        info!("[lossfinder] CrashReopen: dropping sender+receiver and rebuilding");
                        drop(buf);
                        let (w, r) =
                            build_buffer(data_dir.clone(), max_size, WhenFull::Block).await;
                        buf = BufInstance {
                            writer: w,
                            reader: r,
                        };
                    }
                    Scenario::Corruption => {
                        fault_applied = true;
                        // Make sure prior records are durable before corrupting.
                        let _ = buf.writer.flush().await;
                        let mut dats = Vec::new();
                        find_dat_files(&data_dir, &mut dats);
                        if let Some(target) = dats.first() {
                            if let Err(e) = corrupt_file(target) {
                                warn!(error = %e, "[lossfinder] corruption write failed");
                            } else {
                                info!(?target, "[lossfinder] Corruption: flipped one byte");
                            }
                        } else {
                            warn!("[lossfinder] Corruption: no .dat file found yet");
                        }
                    }
                    Scenario::TruncateTail => {
                        fault_applied = true;
                        let _ = buf.writer.flush().await;
                        let mut dats = Vec::new();
                        find_dat_files(&data_dir, &mut dats);
                        if let Some(target) = dats.first() {
                            if let Err(e) = truncate_file(target) {
                                warn!(error = %e, "[lossfinder] truncate failed");
                            } else {
                                info!(?target, "[lossfinder] TruncateTail: truncated tail");
                            }
                        } else {
                            warn!("[lossfinder] TruncateTail: no .dat file found yet");
                        }
                    }
                    _ => {}
                }
            }

            // Per-round sanity asserts.
            assert_always!(
                oracle.delivered <= oracle.produced,
                "lossfinder: never deliver more than produced"
            );
            assert_always!(
                oracle.max_delivered_id <= oracle.produced,
                "lossfinder: every delivered id was previously produced"
            );

            write_status(&status_path, &oracle, scenario);
        }

        // ===================== QUIESCE PHASE ==============================
        // Stop producing, flush, and drain the reader.
        let _ = buf.writer.flush().await;
        let reject = scenario == Scenario::RejectDeliveries;
        let mut empty_streak = 0u32;
        let mut iters = 0u32;
        while empty_streak < 5 && iters < 2000 {
            iters += 1;
            match time::timeout(Duration::from_millis(50), buf.reader.next()).await {
                Ok(Some(mut record)) => {
                    empty_streak = 0;
                    let id = record.id;
                    let finalizers = record.take_finalizers();
                    if reject && rand_in(0, 3) == 0 {
                        finalizers.update_status(EventStatus::Rejected);
                        oracle.rejected += 1;
                    } else {
                        finalizers.update_status(EventStatus::Delivered);
                        oracle.delivered += 1;
                        oracle.max_delivered_id = oracle.max_delivered_id.max(id);
                        oracle.outstanding.remove(&id);
                    }
                    drop(record);
                }
                Ok(None) => {
                    empty_streak += 1;
                }
                Err(_) => {
                    // Timed out waiting for next(): treat as a quiet tick.
                    empty_streak += 1;
                }
            }
        }

        // ===================== CHECK PHASE ================================
        let leftover: BTreeSet<u64> = oracle
            .outstanding
            .difference(&oracle.allowed_loss)
            .copied()
            .collect();

        let scenario_checks = match scenario {
            // drop_newest is best-effort; we do not run a strict oracle here.
            Scenario::DropNewestOverfill => false,
            _ => true,
        };

        if scenario_checks {
            // `assert_always!` requires a static-literal message, so we branch
            // per scenario rather than passing a computed string.
            let ok = leftover.is_empty();
            match scenario {
                Scenario::Baseline => {
                    assert_always!(ok, "lossfinder Baseline: no silent data loss")
                }
                Scenario::WriterDropNoFlush => assert_always!(
                    ok,
                    "lossfinder WriterDropNoFlush: flushed records survive (no silent loss)"
                ),
                Scenario::RejectDeliveries => assert_always!(
                    ok,
                    "lossfinder RejectDeliveries: rejected records retained (no silent loss)"
                ),
                Scenario::CrashReopen => assert_always!(
                    ok,
                    "lossfinder CrashReopen: flushed records survive crash (no silent loss)"
                ),
                Scenario::Corruption => assert_always!(
                    ok,
                    "lossfinder Corruption: no collateral loss of later records (no silent loss)"
                ),
                Scenario::TruncateTail => assert_always!(
                    ok,
                    "lossfinder TruncateTail: no collateral loss of later records (no silent loss)"
                ),
                Scenario::DropNewestOverfill => unreachable!(),
            }
            if !ok {
                oracle.silent_loss_detected += 1;
                let ids: Vec<u64> = leftover.iter().take(64).copied().collect();
                warn!(
                    ?scenario,
                    leftover = leftover.len(),
                    ?ids,
                    "[lossfinder] SILENT DATA LOSS detected"
                );

                // For corruption/truncation, surface the collateral-loss subset
                // explicitly (post-fault ids that vanished).
                if matches!(scenario, Scenario::Corruption | Scenario::TruncateTail) {
                    let collateral: Vec<u64> =
                        leftover.intersection(&post_fault_ids).copied().collect();
                    if !collateral.is_empty() {
                        warn!(
                            count = collateral.len(),
                            "[lossfinder] COLLATERAL loss of post-fault records"
                        );
                    }
                }
            }
        }

        // ----- reset for next scenario ------------------------------------
        // Clear allowed_loss; the outstanding set carries over (it should be
        // empty after a clean check, and any leftover loss is permanent so we
        // don't want to re-flag it forever — drop those ids now).
        oracle.allowed_loss.clear();
        for id in &leftover {
            oracle.outstanding.remove(id);
        }

        // Periodically rotate to a fresh subdir to bound disk usage. Drain first
        // so we don't strand outstanding ids on the abandoned directory.
        if round.is_multiple_of(8) {
            let _ = buf.writer.flush().await;
            // Best-effort final drain of the old buffer.
            let mut empties = 0u32;
            while empties < 5 {
                match time::timeout(Duration::from_millis(30), buf.reader.next()).await {
                    Ok(Some(mut record)) => {
                        empties = 0;
                        let id = record.id;
                        let f = record.take_finalizers();
                        f.update_status(EventStatus::Delivered);
                        oracle.delivered += 1;
                        oracle.max_delivered_id = oracle.max_delivered_id.max(id);
                        oracle.outstanding.remove(&id);
                    }
                    _ => empties += 1,
                }
            }
            subdir_seq += 1;
            let fresh = base_dir.join(format!("run-{subdir_seq}"));
            std::fs::create_dir_all(&fresh).expect("creating fresh subdir should not fail");
            drop(buf);
            let (w, r) = build_buffer(fresh.clone(), max_size, WhenFull::Block).await;
            buf = BufInstance {
                writer: w,
                reader: r,
            };
            data_dir = fresh;
            // Old directory is abandoned; clear any residual oracle state since
            // those ids can never be delivered from the new (empty) buffer.
            oracle.outstanding.clear();
        }

        write_status(&status_path, &oracle, scenario);
    }
}

/// Flip one byte in the middle of a `.dat` file.
fn corrupt_file(path: &Path) -> std::io::Result<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    if len < 4 {
        return Ok(());
    }
    // Pick an offset in the middle third to avoid headers/footers where possible.
    let lo = len / 3;
    let hi = (2 * len) / 3;
    let off = if hi > lo { rand_in(lo, hi) } else { len / 2 };
    f.seek(SeekFrom::Start(off))?;
    let mut byte = [0u8; 1];
    use std::io::Read as _;
    f.read_exact(&mut byte)?;
    byte[0] ^= 0xff;
    f.seek(SeekFrom::Start(off))?;
    f.write_all(&byte)?;
    f.flush()?;
    Ok(())
}

/// Truncate the tail of a `.dat` file to a smaller size.
fn truncate_file(path: &Path) -> std::io::Result<()> {
    let f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    if len < 4 {
        return Ok(());
    }
    // Cut off somewhere in the back half.
    let new_len = rand_in(len / 2, len.max(1));
    f.set_len(new_len)?;
    Ok(())
}

/// Write the per-round status line consumed by the Antithesis observer commands.
fn write_status(path: &Path, oracle: &Oracle, scenario: Scenario) {
    let line = format!(
        "produced={} flushed={} delivered={} rejected={} dropped={} outstanding={} \
         silent_loss={} scenario={:?}\n",
        oracle.produced,
        oracle.produced_flushed,
        oracle.delivered,
        oracle.rejected,
        oracle.dropped_counted,
        oracle.outstanding.len(),
        oracle.silent_loss_detected,
        scenario,
    );
    let _ = std::fs::write(path, line);
}
