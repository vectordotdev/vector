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
- twenty fresh, fault-free records are accepted and delivered after recovery;
- the buffer returns to zero occupancy after those records.

The fresh terminal records each contain a 64 KiB source payload, represented as
a 128 KiB hex field in JSON. Twenty records force a 2 MiB data-file rollover
while every individual record remains below the 256 KiB write-buffer boundary.

The scenario also retains `disk_v2`'s embedded Antithesis checks for record-id
monotonicity, counter underflow, and torn-record accounting. Required
`Sometimes` properties show whether a run reached restart and rollover paths.
Full-buffer blocking and torn-record recovery remain optional paths; failing to
reach them does not fail the run. This experiment intentionally excludes the
large-record path.

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
