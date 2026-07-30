# Disk buffer soundness scenario

This scenario tests properties that `disk_v2` can support today without treating
an HTTP source response as a durable acknowledgement. Fault-phase records may be
lost when Vector is terminated. They are traffic, not conservation obligations.

The topology has three containers: the faulted Vector SUT, an unfaulted oracle,
and a separate test-command workload. Separating the workload from the oracle
gives ingress and egress independent network paths, allowing the buffer to keep
filling while Vector's connection to its sink is faulted.

## Properties

The scenario makes these universal assertions:

- every delivered record has an oracle-issued id and an exact payload;
- logical unread-byte accounting never exceeds the internal buffer capacity;
- recovered unread-byte accounting stays within the configured on-disk limit;
- no individual data file exceeds the configured 2 MiB limit;
- after faults stop, Vector becomes healthy and the existing buffer drains to
  zero event and byte occupancy;
- twelve fresh, fault-free records are accepted and delivered after recovery;
- the buffer returns to zero occupancy after those records.

The fresh terminal records are each just over the 256 KiB write-buffer boundary.
Their JSON representation forces several 2 MiB data-file rollovers, so terminal
progress covers more than a single append to an already-open file.

The scenario also retains `disk_v2`'s embedded Antithesis checks for record-id
monotonicity, counter underflow, torn-record recovery, and full-buffer
backpressure. Their `Sometimes` properties show whether a run reached restart,
rollover, large-record, and full-buffer paths.

## Non-properties

The scenario does not assert that fault-phase records survive. Source
acknowledgements are disabled, and the current end-to-end acknowledgement path
does not acknowledge at the disk buffer's fsync boundary. Claiming conservation
for those records would overstate Vector's durability contract.

Duplicates are permitted. A timed-out HTTP attempt may have entered the pipeline
before the driver retries it.

## Launch

From `tests/antithesis/scenarios`:

```sh
./launch.sh vector_disk_buffer_soundness
```
