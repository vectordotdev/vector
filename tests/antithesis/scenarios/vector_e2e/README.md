# vector_e2e

This is the no-disk counterpart of `vector_to_vector_e2e_disk`: one Vector
process with a memory buffer. It exercises the [shared conservation, integrity,
and liveness properties](../../README.md#shared-test-model) without disk or an
inter-node transport.

## Topology

- `vector` accepts JSON through an `http_server` source on port 8080 and delivers
  it over HTTP to the oracle.
- Its memory buffer uses `when_full: block`, applying backpressure instead of
  dropping events.
- Internal metrics are exposed on port 9598 for the recovery health gate.

## Why a Single Process

Vector's end-to-end acknowledgements are in-process: the source holds the
client's response until every sink that received the event has finished. Here, a
successful producer response means the HTTP sink has already delivered the event
to the oracle. A crash may discard unacknowledged events still in memory, but
those events never became conservation obligations.

## Run

From `tests/antithesis`:

```bash
./scripts/launch.sh vector_e2e
```
