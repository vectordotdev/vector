---
slug: ledger-corruption-no-sigbus-crashloop
catalog_category: 3 — Crash Durability & Recovery
type: Safety / AlwaysOrUnreachable
status: cataloged (Category 7)
related:
  - recovery-completes-after-crash
  - record-id-monotonicity-holds
  - no-corrupted-record-delivered
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

### ledger-corruption-no-sigbus-crashloop — Ledger Corruption Yields a Clean Init Error, Not a SIGBUS Crash Loop

| | |
|---|---|
| **Type** | Safety |
| **Property** | If `buffer.db` is externally truncated or otherwise corrupted before or during Vector startup, the corruption is detected by rkyv `CheckBytes` validation in `BackedArchive::from_backing` and reported as a clean `LedgerLoadCreateError::FailedToDeserialize`, never as a SIGBUS signal mid-operation or as an infinite crash loop. |
| **Invariant** | `AlwaysOrUnreachable`: if the ledger file is corrupted, the process either (a) detects it at `BackedArchive::from_backing` call (`ledger.rs:648`) and returns a `LedgerLoadCreateError`, OR (b) the ledger file is valid and this path is never taken. A SIGBUS-generating memory access against a truncated mmap'd region is `Unreachable` during normal operation; a restart loop (SIGBUS → crash → restart → SIGBUS again) is `Unreachable`. |
| **Antithesis Angle** | Filesystem fault: truncate or zero-fill `buffer.db` while Vector is stopped (before restart) or while it is running (after the file is mmap'd). Assert that Vector (a) either restarts cleanly with a fresh buffer, or (b) emits a detectable error and exits cleanly — it does not loop on SIGBUS signals. Requires Antithesis filesystem-fault capability to truncate a file from outside the process; flag this as potentially unavailable in some tenant configurations. |
| **Why It Matters** | The `buffer.db` ledger is memory-mapped via `memmap2::MmapMut` (`io.rs:161`). There is no SIGBUS handler anywhere in the Vector codebase. If the mapped file is truncated while mapped, any read/write of the now-unmapped pages delivers SIGBUS, which is an unhandled signal and crashes the process. On restart, `open_mmap_writable` re-maps the same truncated file — the same access pattern fires again. The result is an infinite crash loop with no operator-visible error message, indistinguishable from a persistent hardware fault. The theoretical protection (`CheckBytes` at `from_backing`) is only effective at init time, before the mmap is held live. |

---

## What Led to This Property

The SUT analysis (§6 item 9, §8) flagged the mmap'd ledger as a SIGBUS risk.
This evidence file traces the exact code path from ledger load to SIGBUS
exposure and explains why `CheckBytes` is not sufficient as a defense.

### The mmap path in `load_or_create`

`Ledger::load_or_create` (`ledger.rs:583–678`) performs the following sequence:

1. Opens `buffer.db` as a read-write file (`ledger.rs:607–611`).
2. Checks whether the file is empty; if so, writes the serialized default
   `LedgerState` and calls `sync_all` (`ledger.rs:618–638`).
3. Opens the same file as a writable mmap:

   ```rust
   let ledger_mmap = config
       .filesystem
       .open_mmap_writable(&ledger_path)    // ledger.rs:645–646
       .await
       .context(IoSnafu)?;
   ```

   In `ProductionFilesystem`, `open_mmap_writable` opens the file and calls
   `unsafe { memmap2::MmapMut::map_mut(&std_file) }` (`io.rs:157–162`). This
   is the point at which the OS maps the file into the process address space.
4. The mmap is passed to `BackedArchive::from_backing(ledger_mmap)`:

   ```rust
   let ledger_state = match BackedArchive::from_backing(ledger_mmap) {
       Ok(backed) => backed,                           // ledger.rs:648–655
       Err(e) => {
           return Err(LedgerLoadCreateError::FailedToDeserialize {
               reason: e.into_inner(),
           });
       }
   };
   ```

   `from_backing` calls `check_archived_root::<LedgerState>(backing.as_ref())`
   (`backed_archive.rs:73`), which invokes rkyv's `CheckBytes` validation on
   the entire mapped region. If the file is truncated (shorter than
   `LEDGER_LEN = align16(mem::size_of::<ArchivedLedgerState>())`), this read
   will access pages beyond the file end — either via the byte-slice produced
   by `AsRef<[u8]>` on the `MmapMut`, or through a bounds check in memmap2.
   In practice, `memmap2::MmapMut::as_ref()` returns a slice bounded by the
   mmap length (not the file length), so a short file produces a short slice
   and `check_archived_root` may return an error rather than SIGBUS — **at
   init time**.

### Where SIGBUS becomes a live risk

The SIGBUS risk materialises **after** init completes and the `Ledger` struct
holds the live `BackedArchive<MmapMut, LedgerState>` as `self.state`
(`ledger.rs:217`). The ledger fields (`writer_next_record`, `reader_last_record`,
etc.) are `AtomicU64`/`AtomicU16` values overlaid on the mapped region via the
`ArchivedLedgerState` projection (`backed_archive.rs:89`):

```rust
pub fn get_archive_ref(&self) -> &T::Archived {
    unsafe { archived_root::<T>(self.backing.as_ref()) }
}
```

Every ledger field read or write (`get_total_buffer_size`, `increment_pending_acks`,
`flush`, etc.) accesses memory in the mapped region. If the backing file is
truncated **after** the init-time `CheckBytes` validation has passed (i.e.,
while Vector is running), subsequent accesses to the mapped pages produce
SIGBUS. There is no `SIGBUS`/`SIGFPE` signal handler anywhere in the codebase
(confirmed by repo-wide grep). SIGBUS is fatal by default on Linux; the process
is killed.

### The crash-loop path

After a SIGBUS kill, Vector's process supervisor (systemd, Docker restart
policy, Kubernetes) restarts it. On the next startup, `open_mmap_writable`
re-maps `buffer.db`. If the file is still truncated:

- If it is short enough that `check_archived_root` on the resulting
  short mmap slice fails validation → `FailedToDeserialize` error → Vector
  exits cleanly with an error log. **This is the safe path.**
- If the file is truncated to a length that is a multiple of the mmap page
  size but shorter than `LEDGER_LEN` (an edge case on Linux where the OS
  rounds mmap length up to a page boundary), the mmap may succeed and the
  slice appears long enough, `CheckBytes` passes, but accesses to the
  truncated pages that were zero-filled by the OS may yield plausible-looking
  zero data rather than a SIGBUS. Whether this is valid depends on whether
  the rkyv `ArchivedLedgerState` layout treats zero-valued atomics as a
  valid state — the `LedgerState::default()` impl (`ledger.rs:110+`) starts
  all fields at 0, so a zero-filled truncated ledger may appear valid, causing
  Vector to start normally with a reset ledger rather than detecting corruption.
  This "silent reset" is a distinct failure mode from the crash loop.
- If the file is zero-length, the init-time file-is-empty check (`ledger.rs:618`)
  triggers re-initialization with the default state — this is the **correct
  recovery path** and the only case where existing code handles truncation
  gracefully.

### The no-SIGBUS-handler confirmation

```
grep -rn "signal\|SIGBUS\|sigaction\|SignalKind\|unix::signal" \
     lib/vector-buffers/ src/      # 0 relevant matches
```

No SIGBUS handler is installed. The process will receive the default SIGBUS
disposition (terminate + core dump). The behavior on truncation during live
operation is therefore: immediate process death with a SIGBUS, no flush, no
ledger close, no error log.

---

## Code References

| Location | Relevance |
|---|---|
| `lib/vector-buffers/src/variants/disk_v2/io.rs:157–162` | `open_mmap_writable`: `unsafe { memmap2::MmapMut::map_mut(&std_file) }` — the mmap creation point; no SIGBUS guard |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:645–656` | `load_or_create`: mmap opened, then passed to `BackedArchive::from_backing` for `CheckBytes` validation |
| `lib/vector-buffers/src/variants/disk_v2/backed_archive.rs:68–80` | `BackedArchive::from_backing`: calls `check_archived_root` — the only structural validation; only runs at init time |
| `lib/vector-buffers/src/variants/disk_v2/backed_archive.rs:88–91` | `get_archive_ref`: `unsafe { archived_root::<T>(self.backing.as_ref()) }` — live mmap accesses; SIGBUS risk point |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:217` | `state: BackedArchive<FS::MutableMemoryMap, LedgerState>` — the held live mmap |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:253` | `pub fn state(&self) -> &ArchivedLedgerState` — every field access goes through this |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:534–535` | `flush`: `self.state.get_backing_ref().flush()` — calls `MmapMut::flush` (msync); SIGBUS risk if pages are unmapped |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:618` | Zero-length file check — the **only** existing graceful-recovery path for truncation |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:34–75` | `LedgerLoadCreateError` variants — `FailedToDeserialize` is the intended corruption signal |

---

## What Breaks

**Scenario 1 — Corruption before restart (init-time detection, safe path):**
`buffer.db` is written with garbage bytes or truncated to a non-zero, non-page-aligned
length before Vector starts. `check_archived_root` (`backed_archive.rs:73`)
detects the invalid layout and returns `FailedToDeserialize`. Vector logs an
error and exits. No SIGBUS. This is the **intended behavior** and works correctly
as long as the corruption is structurally visible to rkyv.

**Scenario 2 — Corruption to page-aligned truncation (silent reset, unexpected behavior):**
`buffer.db` is truncated to exactly 0 bytes before restart. The zero-length
check at `ledger.rs:618` triggers `LedgerState::default()` initialization and
writes a fresh ledger. Vector starts with a reset ledger, treating all data
files as unknown. This loses the reader's acked position — potentially
re-delivering already-acked records. Unexpected but non-crashing.

**Scenario 3 — Truncation while running (SIGBUS, worst case):**
`buffer.db` is truncated while the live mmap is held. The next
`get_archive_ref()` call (via `state()`) — which happens on every write, every
ack, every flush — accesses a now-unmapped page. SIGBUS is delivered. No error
log; the process dies. Supervisor restarts Vector. Depending on how the file
is left, scenarios 1 or 2 apply on restart. If the file was truncated but not
to zero, scenario 1's `CheckBytes` validation may catch it, giving a clean
error and stopping the crash loop. If the truncation is to zero, scenario 2
applies and the crash loop terminates. If the truncation lands in a
page-aligned-but-structurally-valid range, scenario 2's silent reset may occur.

**Infinite crash loop condition:** the loop is only truly infinite if the file
is truncated in a way that passes `CheckBytes` but still produces a SIGBUS
during runtime access. Given that `check_archived_root` reads the slice
boundaries, this is unlikely for most truncation patterns — but not formally
impossible, particularly given that `CheckBytes` for `LedgerState` is
auto-derived (`ledger.rs:93`) and may not check all cross-field invariants.

---

## Fault Conditions

This property requires **filesystem-fault capability** to truncate or corrupt a
file from outside the process. In Antithesis, this is typically available as a
filesystem-level fault or via a workload container that shares the buffer
volume. However:

- Some Antithesis tenant configurations may restrict filesystem faults on
  non-network volumes.
- **Flag to the user:** confirm that the Antithesis tenant allows
  write/truncate of files in the buffer data directory from a workload
  container or fault injector.
- If filesystem faults are unavailable, this property degrades to a
  "documented risk without test coverage" — still worth cataloging as a
  gap.

A weaker version of this property is testable without filesystem faults:
inject a corrupt `buffer.db` file at container startup (before Vector
starts), using a workload init script. This covers scenario 1 (init-time
detection) but not scenario 3 (live truncation).

---

## SUT Instrumentation

The Antithesis SDK is a committed dependency under the `antithesis` feature, and three `assert_always_greater_than_or_equal_to!` underflow detectors already ship (ledger.rs:271, ledger.rs:313, reader.rs:529 — see `existing-assertions.md`). None of them covers the SIGBUS / ledger-corruption surface, so the assertions below are genuine still-to-add suggestions:

1. **`AlwaysOrUnreachable` assertion** at the mmap-access point in
   `get_archive_ref` (`backed_archive.rs:88`): the function is called in
   contexts where the underlying file's size has not been re-validated since
   init. An assertion here would be logically `Unreachable` for SIGBUS (which
   terminates the process before an assertion fires), but the SUT-side
   instrumentation needed is a **pre-access size check**: before calling
   `archived_root`, assert `self.backing.as_ref().len() >= LEDGER_LEN`. Any
   violation would catch the case where a live mmap has shrunk below the
   required layout size.

2. **Workload-level observation:** a restart after SIGBUS is observable by
   the workload if it monitors the Vector process exit code (SIGBUS = exit
   status 138 on Linux). A pattern of repeated SIGBUS exits is the
   crash-loop signal.

3. **Clean error detection:** an `AlwaysOrUnreachable` assertion in the
   `BackedArchive::from_backing` `Err` arm (`ledger.rs:651–655`) confirms that
   the `FailedToDeserialize` path actually fires on corruption — i.e., that
   the recovery runs, not that it's dead code.

---

## Open Questions

- Does `memmap2::MmapMut::as_ref()` return a slice bounded by the file length
  at mmap time, or by the current file length? If the former, `CheckBytes` at
  init sees the correctly-sized slice even if the file is later grown, but a
  truncation after init is still invisible to the slice bounds. If the latter,
  a live truncation immediately narrows the slice and all in-flight references
  become dangling — which is a soundness issue in the `unsafe archived_root`
  call at `backed_archive.rs:89`. This is an open question about memmap2's
  behavior that determines the exact SIGBUS trigger condition.

- Is there a reason no SIGBUS handler is installed? If the intent is to
  treat SIGBUS as a fatal bug (correct) rather than a recoverable error, then
  the defense must be pre-access validation — the current code has none
  outside of init.

- Does the `LedgerState::default()` zero-fill path (`ledger.rs:110–121`,
  assuming standard derive) produce a structurally valid `ArchivedLedgerState`
  that rkyv's `CheckBytes` will accept? If the all-zeros layout is not a valid
  rkyv archive, then truncation to zero would fail `CheckBytes` and trigger
  `FailedToDeserialize` rather than scenario 2's silent reset. Clarify which
  is the actual behavior.

- Should `load_or_create` validate that `buffer.db` is exactly `LEDGER_LEN`
  bytes before attempting to mmap it, rather than relying on `CheckBytes` to
  catch layout violations? A length mismatch is a stronger, simpler guard
  than rkyv structural validation and would close the gap for all truncation
  scenarios at init time.

- Is there a path where `buffer.db` grows beyond `LEDGER_LEN` (e.g., due to
  an OS-level race or a write to the wrong offset)? If so, `CheckBytes` would
  still pass (the extra bytes are ignored) but the archive root pointer would
  point into the wrong region of the extended file, potentially yielding
  corrupted field values without a detectable error.

---

### Investigation Log

#### Is filesystem-fault injection available in the Antithesis tenant?

**Examined:** Evidence file prose at the "Fault Conditions" section (above), Antithesis documentation (not re-fetched — relying on existing knowledge).

**Not found:** No confirmation in the codebase or local docs that the Antithesis tenant configuration used for Vector testing enables write/truncate filesystem faults on non-network volumes. This capability varies by tenant configuration and must be verified with the Antithesis engagement team.

**Conclusion:** This question requires human input. The property is partially testable without live filesystem faults (inject a corrupt `buffer.db` at container startup before Vector starts, covering Scenario 1 — init-time detection), but Scenario 3 (live truncation while the mmap is held) requires the Antithesis tenant to support filesystem-level fault injection on the buffer data directory. Flag to the Antithesis team for confirmation before relying on this property for live-truncation coverage.

#### Does an all-zeros `LedgerState` pass `CheckBytes` (silent-reset vs. `FailedToDeserialize` on zero-truncation)?

**Examined:** `ledger.rs:618` (zero-length file check), `ledger.rs:110+` (implied `LedgerState::default`), `backed_archive.rs:68–80` (`from_backing` / `check_archived_root`), `ledger.rs:93` (`#[derive(...)]` on `LedgerState`).

**Found:** The zero-length file check at ledger.rs:618 handles the empty-file case before the mmap path is reached — Vector re-initializes with `LedgerState::default()` and writes a fresh ledger. For a non-zero truncation that lands on a page boundary with zero-filled pages (OS behavior on Linux for sparse/truncated mmap regions), `check_archived_root` would validate the all-zeros bytes against the rkyv-archived `LedgerState` layout. Since `LedgerState` fields are all numeric types (AtomicU16, AtomicU64) and rkyv's `CheckBytes` for primitives validates alignment and range — all zeros is a valid representation for all numeric types — the all-zeros layout would likely pass `CheckBytes` and yield a ledger with all fields at 0 (equivalent to a fresh ledger). This means truncation to a page-aligned non-zero length could silently reset the ledger rather than returning `FailedToDeserialize`.

**Not found:** Definitive confirmation of rkyv `CheckBytes` behavior for the exact `ArchivedLedgerState` layout without running the code. The `#[derive(CheckBytes)]` on `LedgerState` is auto-generated; it validates alignment and field validity but does not enforce cross-field invariants (e.g., that writer ID >= reader ID).

**Conclusion:** The all-zeros silent-reset scenario is plausible but not formally confirmed without running the rkyv `check_archived_root` against an all-zeros buffer. This remains a theoretical risk; the definitive answer requires either a unit test or a code trace of the generated `CheckBytes` impl. Flagged as an open sub-question pending code verification or a targeted test.
