# ack-does-not-imply-durability — A 200 Means "Encoded", Not "Durable"

**Cluster:** I (semantic claim/reality divergences) · also bridges A/B/F.
**Type:** Safety (exhibition — expected to FAIL; that failure is the demonstration).

## Claim (as people state it)

"Turn on e2e acks + a disk buffer and Vector is durable: when the `http_server`
returns 200, the event has been safely persisted to disk and will survive a crash."

## Code reality

The 200 is gated on `BatchStatus::Delivered` (`src/sources/util/http/prelude.rs:303-310`).
For the disk path, `Delivered` is produced when the event is **encoded into the
in-memory write buffer**, not when it is fsync'd:

- The source `EventFinalizer` rides in `EventMetadata.finalizers`. The disk write
  consumes the event via `archive_record` → `record.encode(...)` (`writer.rs:472`),
  and `encode` takes the event by value and converts to protobuf types that carry
  **no finalizers** (`vector-core/event/proto.rs`), so the finalizer is **dropped at
  encode time**.
- On drop, the finalizer's default `Dropped` status is *ignored* by
  `BatchNotifier::update_status` and the batch status defaults to `Delivered`
  (`vector-common/src/finalization.rs:243-252,284-289`) → the source sees
  `Delivered` and returns the 200.
- The per-record send path performs **no flush/fsync** (`topology/channel/sender.rs:46-59`); fsync
  happens only on file roll / `should_flush` / explicit flush, none on the hot path.

So at 200 the bytes may sit only in the `TrackingBufWriter` page buffer / OS page
cache, never fsync'd.

## Divergence

**Acked ≠ durable.** A crash or config reload before the next fsync permanently
loses data the client was told (200) was safe.

## How to exhibit

- **Cleanest:** config reload (#24948) — `BufferWriter::Drop` calls `close()` but
  never `flush()`/`sync_all()` (`writer.rs:1366-1374`), so a reload while the write
  buffer holds unflushed acked records drops them. No kernel-level fault needed.
- **Crash variant:** kill node0 before the next file-roll/`should_flush` fsync.
- **Torn-tail variant:** crash mid-write → the unflushed acked tail is skipped on
  reopen (`reader.rs:111-115`, "acknowledged but the data/file was corrupted").
- Producer relays acks per id (one id/invocation); oracle at quiescence:
  `assert_always(missing_count==0)` + `assert_unreachable("an end-to-end-acked event
  was permanently lost")` on the loss branch so Antithesis hunts the schedule.

This is the property the **launched exhibition run** (`48c94f81…`) targets.

## Evidence trail

Grounded by the 2026-06-02 Vector-durability chorus agent against the current tree.
Key sites: `prelude.rs:303-310`, `writer.rs:472`, `finalization.rs:243-252,284-289`,
`topology/channel/sender.rs:46-59`, `writer.rs:1366-1374`, `reader.rs:111-115`.

## Open questions

- Smallest reliable reload cadence vs. produce rate to guarantee unflushed records
  straddle the rebuild. `(tune in harness)`
- Does any topology-level flush run before `BufferWriter::Drop` on reload, narrowing
  the window? `(cross-ref graceful-shutdown-flushes-all; appears not to on reload)`
