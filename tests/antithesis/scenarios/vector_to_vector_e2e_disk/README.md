# vector_to_vector_e2e_disk

This scenario exercises the [shared conservation, integrity, and liveness
properties](../../README.md#shared-test-model) across two Vector nodes with a
`disk_v2` buffer on each hop.

## Topology

- `head` accepts JSON through an `http_server` source and forwards it to `tail`
  over the native Vector protocol. Its `disk_v2` buffer uses `when_full: block`.
- `tail` receives the Vector stream and delivers it over HTTP to the oracle
  through a second blocking `disk_v2` buffer.
- Both nodes expose internal metrics for the recovery health gate and store their
  buffers on separate persistent volumes.

The data-file size is reduced to 2 MiB so files rotate frequently while still
fitting the largest payload class. The total buffer capacity is 8 MiB, making a
stalled reader fill the buffer and expose its lack of progress quickly.

## Why Acknowledged Events Matter

The producer treats a successful response from `head` as an end-to-end
acknowledgement. On this disk path, that acknowledgement can occur after an event
is encoded into the buffer's in-memory writer but before it is fsynced. The
scenario deliberately asks whether acknowledged obligations survive crashes and
other injected faults; an acknowledged id that never reaches the oracle is the
data-loss signal.

The oracle is not terminated or hung because its in-memory obligation ledger is
the source of truth. Head and tail are independently faulted and network faults
exercise both transport links.

## Run

From `tests/antithesis`:

```bash
./scripts/launch.sh vector_to_vector_e2e_disk
```
