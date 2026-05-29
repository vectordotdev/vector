# Property: recovery-completes-after-crash

## Catalog Entry

**Type:** Liveness / Sometimes — `Sometimes(buffer_reinitialized)`

**Property:** `Buffer::from_config_inner` (the full recovery sequence:
`load_or_create` + `validate_last_write` + `seek_to_next_record` +
`synchronize_buffer_usage`) completes successfully and within bounded time after
a kill at any point during normal operation. It does not hang indefinitely, does
not fail fatally, and does not require manual intervention.

**Invariant:** After any SIGKILL followed by a restart, `from_config_inner`
must return `Ok(...)` within T seconds (suggested T = 30s, a reasonable
initialization bound). Neither of the following may occur:

- Permanent hang (e.g., waiting forever on a `ledger.wait_for_reader().await`
  inside `ensure_ready_for_write` called from `validate_last_write`).
- Unrecoverable error returned (crash loop: the process dies during init on
  every restart attempt).

**Antithesis Angle:**

1. Workload runs writes/reads/rotations continuously against the buffer.
2. Antithesis injects SIGKILL at arbitrary points (during write, during
   `flush_inner`, during `sync_all`, during file rotation, during ledger msync,
   during `seek_to_next_record`'s file-deletion loop).
3. Vector restarts. The workload measures time from process start to the first
   event being emittable from the buffer (proxy for `from_config_inner`
   completion).
4. `Sometimes(buffer_reinitialized)` asserts that in at least one timeline,
   `from_config_inner` completes and the writer is ready — i.e., the recovery
   path is actually exercised (not just happy-path startup every time).
5. An `assert_always` (added SUT-side) fires if the initialization hangs beyond
   a timeout or returns an unrecoverable error.

**Why It Matters:** A buffer that silently deadlocks on startup is worse than
one that loses data — it makes Vector permanently unavailable until manual
intervention. The underflow bug (#21683) can cause exactly this: if
`total_buffer_size` wraps to 2^64 during init (via `update_buffer_size` seeding
too high relative to what `seek_to_next_record` decrements), `is_buffer_full()`
returns true forever and `ensure_ready_for_write`'s outer loop at
`writer.rs:1003-1019` spins on `wait_for_reader()` indefinitely — a silent
stall that looks like a healthy process.

**The init sequence (all four steps must complete):**

```
mod.rs:251   Ledger::load_or_create         ← mmap buffer.db, update_buffer_size
mod.rs:257   writer.validate_last_write     ← calls ensure_ready_for_write (deadlock risk!)
mod.rs:265   reader.seek_to_next_record     ← reads/deletes files up to last acked record
mod.rs:270   ledger.synchronize_buffer_usage
```

**Crash Windows and Stall/Hang Risks (code-precise):**

| Kill point | State left on disk | Recovery risk |
|------------|-------------------|---------------|
| During `open_file_writable_atomic` for new data file (`writer.rs:1071`) | New file may or may not exist (atomicity depends on FS) | `AlreadyExists` branch (`writer.rs:1079-1112`) handles this; file_len=0 → treated as owned. OK. |
| After `increment_writer_file_id` (`writer.rs:1138`), before first record written | Ledger says writer is on file N+1, but file N+1 is empty | `validate_last_write`: calls `ensure_ready_for_write`, which opens file N+1 (empty → `data_file_size==0`). `validate_last_write` exits early at `writer.rs:852-855` with `ready_to_write=true`. OK. |
| During `sync_all` of the new file (`writer.rs:1124`) | New file created but not synced; may be 0 bytes on disk | Same as above: empty file → early exit. OK. |
| After `validate_last_write` sets `should_skip_to_next_file=true`, before `reset()`/`mark_for_skip()` | Old file has invalid last record; ledger not yet rolled | `validate_last_write` starts over: `ready_to_write=false` guard (`writer.rs:840`) prevents double-init. `ensure_ready_for_write` opens next file. **L6 edge**: if next file doesn't exist yet and reader hasn't finished the current file, writer must wait (`writer.rs:1153`). Hang risk if reader is also not yet initialized (ordering: writer init completes before reader init starts per `mod.rs:256-268`). |
| During `seek_to_next_record` file-deletion loop (`reader.rs:883`) | Reader deleted some files; ledger partially updated | On restart, `update_buffer_size` sums remaining `.dat` files (correct since deleted files are gone). `seek_to_next_record` resumes. OK in theory. |
| During `update_buffer_size` file-scan (`ledger.rs:674`) | Scan sees partial set of files | Harmless: worst case over-counts (more files than reality due to concurrent creation); `seek_to_next_record` decrements as it reads. Under-count impossible since scan is a snapshot. |

**L6 init-stall edge (highest risk):**
`validate_last_write` detects a bad record → `should_skip_to_next_file = true`
→ `reset()` + `mark_for_skip()` (`writer.rs:983-986`) → `ready_to_write = true`
→ caller eventually calls `write_record` → `ensure_ready_for_write` → tries to
open next file. If the next file (ID `current+1`) does not yet exist AND
reader's `reader_current_data_file_id` == `writer_current_data_file_id` (same
file), the writer's `open_file_writable_atomic` call succeeds (creates the new
file). But if the next file already exists and is non-empty (reader hasn't
finished it), the writer loops on `wait_for_reader()` at `writer.rs:1153`.

Since `validate_last_write` is called during init (before the reader is
`ready_to_read`), and `seek_to_next_record` runs *after* `validate_last_write`
(`mod.rs:265`), the reader has not yet processed any records. So the "next" file
likely does not exist, and `open_file_writable_atomic` should succeed. However,
in the edge case where the writer rolled to file N+1 before the kill and then
the kill happened during the rotation, file N+1 may already exist as a
partially-written or empty file — the `AlreadyExists` branch handles this.

The more dangerous edge: if `update_buffer_size` overseeds `total_buffer_size`
(file-on-disk includes partial/torn bytes beyond actual readable records), and
`seek_to_next_record` does not fully drain the overseeding by the time it
completes, then `is_buffer_full()` may be permanently true at init time. In
that case the write that triggers the deadlock is the first post-init write —
not during init itself — but init "completing" is then followed by an immediate
permanent stall.

**Advisory lock edge:**
`load_or_create` at `ledger.rs:573-576` calls `lock.try_lock()`. On Linux,
`fcntl` advisory locks are per-process; if the old Vector process dies via
SIGKILL, the lock is released by the kernel. However, on some network
filesystems (NFS, CIFS) the lock may not be released immediately after a crash,
causing `try_lock()` to return `LedgerLockAlreadyHeld` and making
`from_config_inner` return an error on every restart attempt until the lock
expires. This is a crash-loop risk on shared/NFS storage.

**Fault Requirements:** Node-termination faults (SIGKILL) required. Kill
specifically during file rotation is the highest-value timing for L6. CPU
throttle during the init sequence is a secondary lever to widen timing windows.

**Antithesis SDK Assertions (SUT-side, to be added):**

```rust
// At the end of from_config_inner (mod.rs:270, after synchronize_buffer_usage):
antithesis_sdk::assert_sometimes!(
    true,
    "buffer_reinitialized: from_config_inner completed successfully after crash",
    json!({
        "writer_next_record": ledger.state().get_next_writer_record_id(),
        "reader_last_record": ledger.state().get_last_reader_record_id(),
        "total_buffer_size": ledger.get_total_buffer_size(),
    })
);

// At the start of update_buffer_size (ledger.rs:653), log seeded size:
antithesis_sdk::assert_always!(
    total_buffer_size < config.max_buffer_size,
    "update_buffer_size: seeded total_buffer_size within configured max_size",
    json!({ "total_buffer_size": total_buffer_size, "max_buffer_size": config.max_buffer_size })
);
```

---

## Open Questions

**OQ-1: Is the `total_buffer_size` underflow during init detectable at init
time, or only at the first write?**
`update_buffer_size` increments `total_buffer_size` by file sizes. If the sum
exceeds `max_buffer_size`, `is_buffer_full()` returns true immediately, and
the first call to `ensure_ready_for_write` from `validate_last_write` hangs.
But `ensure_ready_for_write` has a guard: `!self.ready_to_write` allows passing
through even if full (`writer.rs:1009`). So the deadlock only manifests after
`validate_last_write` sets `ready_to_write = true` and normal writes begin.
This means init completes but the buffer is immediately broken — `from_config_inner`
returns `Ok` but the system is deadlocked. Confirm by tracing
`is_buffer_full()` at the `ensure_ready_for_write` init call vs. the post-init
write path.

**OQ-2: Is there a bounded-time guarantee on `seek_to_next_record`?**
`seek_to_next_record` reads records via `next()` (`reader.rs:905`). `next()`
can block waiting for the writer (`reader.rs:1019` `wait_for_writer()`). During
init the writer is not yet writing, so `wait_for_writer()` may hang. Trace the
`next()` path under `is_finalized = (reader_file_id != writer_file_id) ||
!self.ready_to_read` (`reader.rs:1004`). During seek, `ready_to_read = false`,
so `is_finalized = true`, so `try_next_record` treats the file as finalized and
returns `PartialWrite` errors rather than blocking. Confirm this holds for all
files in the seek path, not just the current writer file.

**OQ-3: What happens if `validate_last_write` returns an error?**
`from_config_inner` propagates the error as `BufferError::WriterSeekFailed`
(`mod.rs:260`). The caller (topology builder) treats this as fatal and does not
retry. If the error is transient (e.g., a temp I/O error on the `open_mmap_readable`
call inside `validate_last_write` at `writer.rs:862-867`), the process crash-
loops. Antithesis should inject filesystem errors (EIO, ENOENT on the data file)
to verify whether crash-loop vs. graceful degradation is the actual behavior.

**OQ-4: Advisory lock on NFS — is Vector deployed on NFS-backed storage in
any customer environment?**
If so, the lock-not-released edge is a live operational risk, not just a
theoretical one. Flag to the user.
