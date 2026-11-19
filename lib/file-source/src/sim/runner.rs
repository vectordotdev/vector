use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use async_compression::tokio::bufread::GzipEncoder;
use bytes::Bytes;
use file_source_common::{
    Checkpointer, FileFingerprint, FingerprintStrategy, Fingerprinter, ReadFrom,
};
use proptest::prelude::*;
use tokio::io::AsyncReadExt as _;

use crate::{
    file_server::{FileServer, FileServerState, Line},
    scheduler::{
        AdaptiveEventScheduler, AdaptiveSchedulerConfig, FsEventSource, LegacyPollingScheduler,
        NoopFsEventSource, ReadScheduler,
    },
};

use super::{
    action::{DstConfig, NUM_SLOTS, SimAction},
    clock::SimClock,
    emitter::RecordingEmitter,
    fs::{EventDelivery, SimFileSystem, SimPathsProvider},
};

#[derive(Default)]
struct StreamModel {
    /// Complete non-oversized lines written to this stream, in order.
    expected_lines: Vec<String>,
    /// Incomplete bytes written without a trailing `\n` yet.
    pending_partial: String,
    /// Total newlines written (including oversized lines), used to determine
    /// whether `FirstLinesChecksum { lines }` threshold was reached.
    total_newlines: usize,
    /// True if the file was unlinked or moved outside the watched glob before a
    /// `Restart` (in which case a fresh process cannot re-open it).
    lost_across_restart: bool,
    /// True if the file was unlinked or moved outside the watched glob before
    /// `FileServer` ever discovered it.
    unfindable_before_discovery: bool,
}

#[derive(Clone)]
struct SlotEntry {
    path: PathBuf,
    ino: u64,
    is_symlink: bool,
    is_gzip: bool,
}

/// Deterministic simulation runner executing a `proptest`-generated [`SimAction`]
/// trace against [`FileServerState`] + [`SimFileSystem`] + [`SimClock`].
pub struct DstRunner<
    S: ReadScheduler = LegacyPollingScheduler,
    Ev: FsEventSource = NoopFsEventSource,
> {
    config: DstConfig,
    clock: SimClock,
    fs: SimFileSystem,
    emitter: RecordingEmitter,
    checkpointer: Checkpointer<SimFileSystem, SimClock>,
    state:
        Option<FileServerState<SimPathsProvider, RecordingEmitter, SimFileSystem, SimClock, S, Ev>>,
    make_scheduler: Box<dyn Fn(&DstConfig) -> S + Send>,
    make_event_source: Box<dyn Fn(&SimFileSystem) -> Ev + Send>,
    active_slots: Vec<Option<SlotEntry>>,
    /// Rotated files that remain inside `/logs/*` and can still receive appends.
    rotated_in_glob: Vec<(PathBuf, u64)>,
    /// Files moved outside `/logs/*` or unlinked (cannot be re-opened after `Restart`).
    unfindable_inos: HashSet<u64>,
    /// Paths currently marked unreadable (`EACCES`).
    unreadable_paths: HashSet<PathBuf>,
    models: BTreeMap<u64, StreamModel>,
    /// Delivered lines per stream across the entire run (may include at-least-once
    /// redeliveries after an unacked or crashed-checkpoint restart).
    delivered_by_ino: HashMap<u64, Vec<String>>,
    /// Per-stream `end_offset` high-water mark within the *current* server epoch
    /// (must be strictly monotonic within an epoch).
    epoch_offset_by_ino: HashMap<u64, u64>,
    /// Mapping from `FileFingerprint` -> `ino` learned from tagged lines or `DevInode`.
    fp_to_ino: HashMap<FileFingerprint, u64>,
    /// Lines emitted by `FileServer` that have not yet been acknowledged into `CheckpointsView`.
    pending_acks: Vec<(FileFingerprint, u64)>,
    /// In-memory count of restarts that occurred with unacked or unpersisted progress.
    had_unpersisted_restart: bool,
    /// True if any checkpoint updates have occurred since the last clean `CheckpointFlush`.
    dirty_checkpoints: bool,
    line_counter: u64,
    rot_counter: u64,
}

impl<S: ReadScheduler, Ev: FsEventSource> DstRunner<S, Ev> {
    pub async fn new_with(
        config: DstConfig,
        delivery: EventDelivery,
        make_scheduler: impl Fn(&DstConfig) -> S + Send + 'static,
        make_event_source: impl Fn(&SimFileSystem) -> Ev + Send + 'static,
    ) -> Self {
        let clock = SimClock::new();
        let fs = SimFileSystem::new(clock.clone());
        fs.set_max_bytes_per_read(config.max_bytes_per_read);
        fs.set_event_delivery(delivery);
        let emitter = RecordingEmitter::new();
        let mut checkpointer =
            Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
        let server = build_server(&fs, emitter.clone(), &config);
        let scheduler = make_scheduler(&config);
        let event_source = make_event_source(&fs);
        let state = FileServerState::init_with_scheduler(
            server,
            &mut checkpointer,
            fs.clone(),
            clock.clone(),
            scheduler,
            event_source,
        )
        .await;

        Self {
            config,
            clock,
            fs,
            emitter,
            checkpointer,
            state: Some(state),
            make_scheduler: Box::new(make_scheduler),
            make_event_source: Box::new(make_event_source),
            active_slots: vec![None; NUM_SLOTS],
            rotated_in_glob: Vec::new(),
            unfindable_inos: HashSet::new(),
            unreadable_paths: HashSet::new(),
            models: BTreeMap::new(),
            delivered_by_ino: HashMap::new(),
            epoch_offset_by_ino: HashMap::new(),
            fp_to_ino: HashMap::new(),
            pending_acks: Vec::new(),
            had_unpersisted_restart: false,
            dirty_checkpoints: false,
            line_counter: 0,
            rot_counter: 0,
        }
    }

    fn min_fingerprint_lines(&self) -> usize {
        match self.config.strategy {
            FingerprintStrategy::DevInode => 0,
            FingerprintStrategy::FirstLinesChecksum { lines, .. } => lines,
        }
    }

    async fn drain_step(&mut self) -> Result<(), TestCaseError> {
        let mut batch = Vec::new();
        let state = self.state.as_mut().expect("server running");
        let _ = state.step(&mut batch).await;
        for line in batch {
            self.pending_acks.push((line.file_id, line.end_offset));
            self.dirty_checkpoints = true;

            let text = String::from_utf8(line.text.to_vec())
                .map_err(|e| TestCaseError::fail(format!("invalid utf8: {e}")))?;
            let ino = self.resolve_line_ino(&line, &text)?;

            // Within a single server epoch (no restart), offsets for a stream must never go backward.
            if let Some(&prev_end) = self.epoch_offset_by_ino.get(&ino) {
                prop_assert!(
                    line.end_offset >= prev_end,
                    "ino={}: offset regressed within epoch ({} < {})",
                    ino,
                    line.end_offset,
                    prev_end
                );
            }
            self.epoch_offset_by_ino.insert(ino, line.end_offset);
            self.delivered_by_ino.entry(ino).or_default().push(text);
        }
        Ok(())
    }

    fn resolve_line_ino(&mut self, line: &Line, text: &str) -> Result<u64, TestCaseError> {
        if let FileFingerprint::DevInode(_, ino) = line.file_id {
            self.fp_to_ino.entry(line.file_id).or_insert(ino);
            return Ok(ino);
        }
        if let Some(ino) = text
            .strip_prefix("ino=")
            .and_then(|rest| rest.split(':').next())
            .and_then(|id| id.parse::<u64>().ok())
        {
            self.fp_to_ino.entry(line.file_id).or_insert(ino);
            return Ok(ino);
        }
        if let Some(&ino) = self.fp_to_ino.get(&line.file_id) {
            return Ok(ino);
        }
        Err(TestCaseError::fail(format!(
            "could not attribute line {text:?} (fp={:?}) to any stream",
            line.file_id
        )))
    }

    fn ack_pending(&mut self) {
        let state = self.state.as_ref().expect("server running");
        for (file_id, end_offset) in self.pending_acks.drain(..) {
            state.checkpoints().update(file_id, end_offset);
        }
    }

    fn ensure_plain_slot(&mut self, slot: usize) -> Option<(PathBuf, u64)> {
        if let Some(entry) = &self.active_slots[slot] {
            if entry.is_gzip {
                return None;
            }
            return Some((entry.path.clone(), entry.ino));
        }
        let path = PathBuf::from(format!("/logs/slot_{slot}.log"));
        let ino = self.fs.create_file(&path);
        self.active_slots[slot] = Some(SlotEntry {
            path: path.clone(),
            ino,
            is_symlink: false,
            is_gzip: false,
        });
        self.models.entry(ino).or_default();
        Some((path, ino))
    }

    fn finish_partial(&mut self, path: &PathBuf, ino: u64) {
        let model = self.models.get_mut(&ino).unwrap();
        if !model.pending_partial.is_empty() {
            self.fs.append_bytes(path, b"\n");
            let completed = std::mem::take(&mut model.pending_partial);
            model.expected_lines.push(completed);
            model.total_newlines += 1;
        }
    }

    fn append_line_to_stream(&mut self, path: &PathBuf, ino: u64) {
        let model = self.models.get_mut(&ino).unwrap();
        self.line_counter += 1;
        let payload = if model.pending_partial.is_empty() {
            format!("ino={ino}:seq={}", self.line_counter)
        } else {
            format!("_cont_{}", self.line_counter)
        };
        self.fs
            .append_bytes(path, format!("{payload}\n").as_bytes());
        let full_line = format!("{}{payload}", model.pending_partial);
        model.pending_partial.clear();
        model.expected_lines.push(full_line);
        model.total_newlines += 1;
    }

    async fn create_gzip_in_slot(&mut self, slot: usize, members: u8) {
        // Known bug (pinned by `zoo_is_gzipped_fails_on_one_byte_short_read`):
        // `FileWatcher`'s `is_gzipped()` uses a single `fill_buf()` call, so a
        // 1-byte short read misclassifies valid gzip files as plain text.
        if self.config.max_bytes_per_read == Some(1) {
            return;
        }
        if self.active_slots[slot].is_some() {
            return;
        }
        self.rot_counter += 1;
        let path = PathBuf::from(format!("/logs/slot_{slot}_{}.gz", self.rot_counter));
        let ino = self.fs.create_file(&path);
        let mut expected = Vec::new();
        let mut compressed = Vec::new();
        for m in 0..members {
            let mut member_plain = String::new();
            // Write 2 lines per member so even `FirstLinesChecksum { lines: 2 }` fingerprints it.
            for k in 0..2 {
                self.line_counter += 1;
                let line = format!("ino={ino}:gz_m{m}_l{k}_seq={}", self.line_counter);
                member_plain.push_str(&line);
                member_plain.push('\n');
                expected.push(line);
            }
            let mut out = Vec::new();
            GzipEncoder::new(member_plain.as_bytes())
                .read_to_end(&mut out)
                .await
                .unwrap();
            compressed.extend_from_slice(&out);
        }
        self.fs.append_bytes(&path, &compressed);
        self.models.insert(
            ino,
            StreamModel {
                total_newlines: expected.len(),
                expected_lines: expected,
                pending_partial: String::new(),
                lost_across_restart: false,
                unfindable_before_discovery: false,
            },
        );
        self.active_slots[slot] = Some(SlotEntry {
            path,
            ino,
            is_symlink: false,
            is_gzip: true,
        });
    }

    async fn rotate_slot(&mut self, slot: usize, outside_glob: bool) -> Result<(), TestCaseError> {
        let Some(entry) = self.active_slots[slot].take() else {
            return Ok(());
        };
        if entry.is_symlink || entry.is_gzip || self.unreadable_paths.contains(&entry.path) {
            self.active_slots[slot] = Some(entry);
            return Ok(());
        }
        self.finish_partial(&entry.path, entry.ino);
        if outside_glob {
            // Ensure discovery before moving outside the glob so the watcher has an open fd.
            self.clock.advance(Duration::from_secs(1));
            self.drain_step().await?;
        }
        self.rot_counter += 1;
        let rotated = if outside_glob {
            PathBuf::from(format!("/archive/slot_{slot}.log.{}", self.rot_counter))
        } else {
            PathBuf::from(format!("/logs/slot_{slot}.log.{}", self.rot_counter))
        };
        self.fs.rename_file(&entry.path, &rotated).unwrap();
        if outside_glob {
            self.unfindable_inos.insert(entry.ino);
            if self.models[&entry.ino].total_newlines < self.min_fingerprint_lines() {
                self.models
                    .get_mut(&entry.ino)
                    .unwrap()
                    .unfindable_before_discovery = true;
            }
        } else {
            self.rotated_in_glob.push((rotated, entry.ino));
        }
        let new_ino = self.fs.create_file(&entry.path);
        self.active_slots[slot] = Some(SlotEntry {
            path: entry.path,
            ino: new_ino,
            is_symlink: false,
            is_gzip: false,
        });
        self.models.entry(new_ino).or_default();
        Ok(())
    }

    async fn symlink_rotate_slot(&mut self, slot: usize) -> Result<(), TestCaseError> {
        if let Some(entry) = &self.active_slots[slot]
            && (!entry.is_symlink || self.unreadable_paths.contains(&entry.path))
        {
            return Ok(());
        }
        if let Some(old) = self.active_slots[slot].take() {
            self.finish_partial(&old.path, old.ino);
            // Ensure discovery before retargeting symlink away from old target.
            self.clock.advance(Duration::from_secs(1));
            self.drain_step().await?;
            // Old target lives under `/targets/...` (outside `/logs/*`).
            self.unfindable_inos.insert(old.ino);
            if self.models[&old.ino].total_newlines < self.min_fingerprint_lines() {
                self.models
                    .get_mut(&old.ino)
                    .unwrap()
                    .unfindable_before_discovery = true;
            }
        }
        self.rot_counter += 1;
        let link = PathBuf::from(format!("/logs/sym_{slot}.log"));
        let target = PathBuf::from(format!("/targets/sym_{slot}_{}.log", self.rot_counter));
        let ino = self.fs.create_file(&target);
        self.fs.set_symlink(&link, &target);
        self.models.entry(ino).or_default();
        self.active_slots[slot] = Some(SlotEntry {
            path: link,
            ino,
            is_symlink: true,
            is_gzip: false,
        });
        Ok(())
    }

    async fn restart_server(&mut self, step_idx: usize) -> Result<(), TestCaseError> {
        if !self.pending_acks.is_empty() || self.dirty_checkpoints {
            self.had_unpersisted_restart = true;
        }
        self.pending_acks.clear();
        self.state = None;
        prop_assert_eq!(
            self.fs.active_open_handles(),
            0,
            "step={}: fd leak after FileServer drop",
            step_idx
        );

        // Any file that is unlinked or moved outside `/logs/*` cannot be re-opened by
        // a brand-new FileServer process. Also, gzip files with an existing persisted
        // checkpoint are opened as `FileReader::Null` on restart (pinned by
        // `zoo_gzip_partial_checkpoint_restart_drops_remainder`).
        for &ino in &self.unfindable_inos {
            if let Some(model) = self.models.get_mut(&ino) {
                model.lost_across_restart = true;
            }
        }
        for entry in self.active_slots.iter().flatten() {
            if entry.is_gzip
                && self.delivered_by_ino.contains_key(&entry.ino)
                && let Some(model) = self.models.get_mut(&entry.ino)
            {
                model.lost_across_restart = true;
            }
        }

        self.epoch_offset_by_ino.clear();
        let mut new_cp =
            Checkpointer::new_with(&PathBuf::from("/data"), self.fs.clone(), self.clock.clone());
        let new_server = build_server(&self.fs, self.emitter.clone(), &self.config);
        let scheduler = (self.make_scheduler)(&self.config);
        let event_source = (self.make_event_source)(&self.fs);
        self.state = Some(
            FileServerState::init_with_scheduler(
                new_server,
                &mut new_cp,
                self.fs.clone(),
                self.clock.clone(),
                scheduler,
                event_source,
            )
            .await,
        );
        self.checkpointer = new_cp;
        Ok(())
    }

    fn append_empty_line_in_slot(&mut self, slot: usize) {
        if let Some(entry) = &self.active_slots[slot]
            && !entry.is_gzip
        {
            let (path, ino) = (entry.path.clone(), entry.ino);
            let model = self.models.get_mut(&ino).unwrap();
            // Only emit empty lines after at least 1 tagged line so the stream's
            // fingerprint is already mapped to `ino`, and not mid-partial-line.
            if !model.expected_lines.is_empty() && model.pending_partial.is_empty() {
                self.fs.append_bytes(&path, b"\n");
                model.expected_lines.push(String::new());
                model.total_newlines += 1;
            }
        }
    }

    fn append_partial_in_slot(&mut self, slot: usize) {
        if let Some(entry) = &self.active_slots[slot]
            && !entry.is_gzip
            && !self.unreadable_paths.contains(&entry.path)
        {
            let (path, ino) = (entry.path.clone(), entry.ino);
            let model = self.models.get_mut(&ino).unwrap();
            let chunk = if model.pending_partial.is_empty() {
                self.line_counter += 1;
                format!("ino={ino}:part={}", self.line_counter)
            } else {
                "_mid".to_string()
            };
            self.fs.append_bytes(&path, chunk.as_bytes());
            model.pending_partial.push_str(&chunk);
        }
    }

    fn append_oversized_in_slot(&mut self, slot: usize) {
        if let Some((path, ino)) = self.ensure_plain_slot(slot) {
            let model = self.models.get_mut(&ino).unwrap();
            if model.pending_partial.is_empty() {
                self.line_counter += 1;
                let prefix = format!("ino={ino}:big={}:", self.line_counter);
                let pad_len = self.config.max_line_bytes + 32;
                let mut oversized = prefix.into_bytes();
                oversized.resize(oversized.len() + pad_len, b'X');
                oversized.push(b'\n');
                self.fs.append_bytes(&path, &oversized);
                model.total_newlines += 1;
            }
        }
    }

    async fn unlink_slot(&mut self, slot: usize) -> Result<(), TestCaseError> {
        if let Some(entry) = self.active_slots[slot].take() {
            if !entry.is_gzip {
                self.finish_partial(&entry.path, entry.ino);
            }
            if self.unreadable_paths.remove(&entry.path) {
                self.fs.set_readable(&entry.path, true);
                self.epoch_offset_by_ino.remove(&entry.ino);
            }
            self.clock.advance(Duration::from_secs(1));
            self.drain_step().await?;
            self.fs.unlink_file(&entry.path).unwrap();
            self.unfindable_inos.insert(entry.ino);
            if self.models[&entry.ino].total_newlines < self.min_fingerprint_lines() {
                self.models
                    .get_mut(&entry.ino)
                    .unwrap()
                    .unfindable_before_discovery = true;
            }
        }
        Ok(())
    }

    fn toggle_readable_slot(&mut self, slot: usize) {
        if let Some(entry) = &self.active_slots[slot] {
            let path = entry.path.clone();
            let ino = entry.ino;
            if self.unreadable_paths.remove(&path) {
                self.fs.set_readable(&path, true);
                self.epoch_offset_by_ino.remove(&ino);
            } else {
                // Known bug (pinned by `zoo_transient_eacces_tears_partial_line`):
                // EACCES while a partial line is buffered causes `!file_findable()`
                // to flush the incomplete line without a newline.
                if !self.models[&ino].pending_partial.is_empty() {
                    return;
                }
                self.had_unpersisted_restart = true;
                self.unreadable_paths.insert(path.clone());
                self.fs.set_readable(&path, false);
            }
        }
    }

    pub async fn execute_action(
        &mut self,
        step_idx: usize,
        action: &SimAction,
    ) -> Result<(), TestCaseError> {
        match *action {
            SimAction::AppendLine { slot } => {
                if let Some((path, ino)) = self.ensure_plain_slot(slot) {
                    self.append_line_to_stream(&path, ino);
                }
            }
            SimAction::AppendEmptyLine { slot } => self.append_empty_line_in_slot(slot),
            SimAction::AppendPartial { slot } => self.append_partial_in_slot(slot),
            SimAction::AppendOversizedLine { slot } => self.append_oversized_in_slot(slot),
            SimAction::AppendToRotated { rotated_idx } => {
                if !self.rotated_in_glob.is_empty() {
                    let idx = rotated_idx % self.rotated_in_glob.len();
                    let (path, ino) = self.rotated_in_glob[idx].clone();
                    self.append_line_to_stream(&path, ino);
                }
            }
            SimAction::CreateEmptyFile { slot } => {
                if self.active_slots[slot].is_none() {
                    let path = PathBuf::from(format!("/logs/slot_{slot}.log"));
                    let ino = self.fs.create_file(&path);
                    self.active_slots[slot] = Some(SlotEntry {
                        path,
                        ino,
                        is_symlink: false,
                        is_gzip: false,
                    });
                    self.models.entry(ino).or_default();
                }
            }
            SimAction::CreateGzipFile { slot, members } => {
                self.create_gzip_in_slot(slot, members).await;
            }
            SimAction::Rotate { slot } => self.rotate_slot(slot, false).await?,
            SimAction::RotateOutsideGlob { slot } => self.rotate_slot(slot, true).await?,
            SimAction::SymlinkRotate { slot } => self.symlink_rotate_slot(slot).await?,
            SimAction::Unlink { slot } => self.unlink_slot(slot).await?,
            SimAction::ToggleReadable { slot } => self.toggle_readable_slot(slot),
            SimAction::Step { advance_ms } => {
                self.clock.advance(Duration::from_millis(advance_ms));
                self.drain_step().await?;
            }
            SimAction::AdvanceClock { advance_secs } => {
                self.clock.advance(Duration::from_secs(advance_secs));
            }
            SimAction::AckPending => self.ack_pending(),
            SimAction::CheckpointFlush { crash_bytes } => {
                if let Some(n) = crash_bytes {
                    self.fs.inject_partial_write_crash(Some(n));
                    drop(self.checkpointer.write_checkpoints().await);
                    // Clear in case `write_checkpoints` was a no-op (state unchanged).
                    self.fs.inject_partial_write_crash(None);
                } else {
                    self.checkpointer
                        .write_checkpoints()
                        .await
                        .map_err(|e| TestCaseError::fail(format!("write_checkpoints: {e}")))?;
                    if self.pending_acks.is_empty() {
                        self.dirty_checkpoints = false;
                    }
                }
            }
            SimAction::Restart => self.restart_server(step_idx).await?,
        }

        // Open OS handles must never exceed active watchers (watchers using
        // `FileReader::Null` for already-checkpointed gzip files hold 0 handles).
        let open_count = self.state.as_ref().unwrap().open_file_count();
        prop_assert!(
            self.fs.active_open_handles() <= open_count,
            "step={}: active_open_handles ({}) > open_file_count ({})",
            step_idx,
            self.fs.active_open_handles(),
            open_count
        );
        Ok(())
    }

    pub async fn finish_and_verify(mut self) -> Result<(), TestCaseError> {
        // Restore readability on all files so transient EACCES recovers cleanly.
        for path in self.unreadable_paths.drain() {
            self.fs.set_readable(&path, true);
        }
        self.epoch_offset_by_ino.clear();

        // Finish any pending partial lines on active slots.
        let slots = std::mem::take(&mut self.active_slots);
        for entry in slots.into_iter().flatten() {
            if !entry.is_gzip {
                self.finish_partial(&entry.path, entry.ino);
            }
        }

        // Drain thoroughly (enough steps to clear small `max_read_bytes`, `oldest_first`,
        // 10s idle cliff, and glob cooldown).
        for _ in 0..40 {
            self.clock.advance(Duration::from_secs(11));
            self.drain_step().await?;
            self.ack_pending();
        }

        let min_lines = self.min_fingerprint_lines();
        for (ino, model) in &self.models {
            let got = self
                .delivered_by_ino
                .get(ino)
                .map_or(&[][..], Vec::as_slice);

            if model.total_newlines < min_lines || model.unfindable_before_discovery {
                // File never accumulated enough lines to be fingerprinted under `lines: N`.
                continue;
            }

            if model.lost_across_restart {
                // Unlinked/moved outside glob before a restart: every line we DID deliver
                // must be a valid line from this stream (never corrupted/invented).
                for line in got {
                    prop_assert!(
                        model.expected_lines.contains(line),
                        "ino={}: unexpected line {:?} not in expected {:?}",
                        ino,
                        line,
                        model.expected_lines
                    );
                }
                continue;
            }

            if self.had_unpersisted_restart {
                // At-least-once across unacked/crashed-checkpoint restarts:
                // 1. Every delivered line belongs to this stream.
                // 2. Every expected line was delivered at least once, and the deduplicated
                //    first-appearance order matches `expected_lines` exactly!
                let mut first_seen_order = Vec::new();
                let mut seen = HashSet::new();
                for line in got {
                    prop_assert!(
                        model.expected_lines.contains(line),
                        "ino={}: unexpected line {:?}",
                        ino,
                        line
                    );
                    // Only track non-empty lines for first-seen order (empty lines can repeat).
                    if !line.is_empty() && seen.insert(line.clone()) {
                        first_seen_order.push(line.clone());
                    }
                }
                let expected_non_empty: Vec<String> = model
                    .expected_lines
                    .iter()
                    .filter(|l| !l.is_empty())
                    .cloned()
                    .collect();
                prop_assert_eq!(
                    first_seen_order,
                    expected_non_empty,
                    "ino={}: at-least-once order/completeness violated after unpersisted restart",
                    ino
                );
            } else {
                // No unpersisted restart occurred: strict exact delivery (no loss, no dupes).
                prop_assert_eq!(
                    got,
                    model.expected_lines.as_slice(),
                    "ino={}: delivered lines diverged from ground truth",
                    ino
                );
            }
        }

        self.state = None;
        prop_assert_eq!(
            self.fs.active_open_handles(),
            0,
            "fd leak at end of simulation"
        );
        Ok(())
    }
}

fn build_server(
    fs: &SimFileSystem,
    emitter: RecordingEmitter,
    config: &DstConfig,
) -> FileServer<SimPathsProvider, RecordingEmitter> {
    FileServer {
        paths_provider: fs.paths_provider(&["/logs/*"], &[]),
        max_read_bytes: config.max_read_bytes,
        ignore_checkpoints: false,
        read_from: ReadFrom::Beginning,
        ignore_before: None,
        max_line_bytes: config.max_line_bytes,
        line_delimiter: Bytes::from_static(b"\n"),
        data_dir: PathBuf::from("/data"),
        glob_minimum_cooldown: Duration::from_secs(1),
        fingerprinter: Fingerprinter::new(config.strategy.clone(), config.max_line_bytes, false),
        oldest_first: config.oldest_first,
        remove_after: config.remove_after_secs.map(Duration::from_secs),
        emitter,
        rotate_wait: Duration::from_secs(config.rotate_wait_secs),
    }
}

/// Runs a `proptest`-generated `(DstConfig, Vec<SimAction>)` scenario against
/// `FileServerState` with [`LegacyPollingScheduler`].
pub fn run_dst_scenario(config: DstConfig, actions: &[SimAction]) -> Result<(), TestCaseError> {
    run_dst_with(
        config,
        actions,
        EventDelivery::Normal,
        |_| LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
    )
}

/// Runs a `proptest`-generated `(DstConfig, Vec<SimAction>)` scenario against
/// `FileServerState` with [`AdaptiveEventScheduler`] + [`SimFsEventSource`].
pub fn run_dst_scenario_adaptive(
    config: DstConfig,
    actions: &[SimAction],
    delivery: EventDelivery,
) -> Result<(), TestCaseError> {
    run_dst_with(
        config,
        actions,
        delivery,
        |cfg| {
            AdaptiveEventScheduler::new(AdaptiveSchedulerConfig {
                oldest_first: cfg.oldest_first,
                ..AdaptiveSchedulerConfig::default()
            })
        },
        SimFileSystem::event_source,
    )
}

fn run_dst_with<S: ReadScheduler, Ev: FsEventSource>(
    config: DstConfig,
    actions: &[SimAction],
    delivery: EventDelivery,
    make_scheduler: impl Fn(&DstConfig) -> S + Send + 'static,
    make_event_source: impl Fn(&SimFileSystem) -> Ev + Send + 'static,
) -> Result<(), TestCaseError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("build current-thread tokio runtime");
    rt.block_on(async move {
        let mut runner =
            DstRunner::new_with(config, delivery, make_scheduler, make_event_source).await;
        for (step_idx, action) in actions.iter().enumerate() {
            runner.execute_action(step_idx, action).await?;
        }
        runner.finish_and_verify().await
    })
}
