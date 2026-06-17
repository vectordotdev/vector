# vector_e2e

The no-disk counterpart of `vector_to_vector_e2e_disk`. Same two properties, one
Vector process, memory buffer instead of `disk_v2`.

**Conservation**: every event the oracle acked must eventually come back, across
arbitrary Antithesis faults. Duplicates are allowed because the contract is
at-least-once. A missing acked id is the bug.

**Liveness**: once faults stop, the node must still accept fresh writes.

## Why a single process

Vector's end-to-end acknowledgements are in-process: a source holds the client's
ack until every sink that received the event has finished. So a single node is the
honest place to test what an e2e ack promises. The producer POSTs to the node's
`http_server` source; the 200 comes back only once the `http` sink has delivered
the event to the oracle and the oracle returned 2xx. That means an acked id has
**already** reached the oracle — which is why conservation can hold even though the
node has no disk buffer and a crash drops whatever is still in memory: those
in-flight events were never acked, so they were never an obligation.

## How it works

One Vector node and one oracle container.

- **vector** takes an `http_server` source (`:8080`) and delivers over `http` to
  the oracle through an in-memory buffer with `when_full: block` and e2e acks. It
  also exposes Prometheus metrics (`:9598`) for the health gate, and runs the
  reload fault: an `anytime_` command swaps `vector.yaml`/`vector.b.yaml` and sends
  `SIGHUP`, forcing the sink to rebuild mid-run.
- **oracle** (`:8686`) is one container that injects unique event ids at the node
  and runs the HTTP endpoint the node's sink delivers back to.

The oracle keeps its id sets in memory and Antithesis never terminates it, so the
faults under test cannot corrupt the judge. The workload binaries (`oracle`,
`parallel_driver_produce`, `eventually_conservation`) are the shared, buffer-
agnostic bins from `tests/antithesis/harness`, pointed at this topology by the
environment in `docker-compose.yaml`.

## Run

Validate the config locally:

```bash
cd tests/antithesis
docker compose -f scenarios/vector_e2e/docker-compose.yaml build
snouty validate scenarios/vector_e2e
```

Submit a run through the shared launcher, which pins the fault profile (see
`tests/antithesis/AGENTS.md`):

```bash
cd tests/antithesis/scenarios
./launch.sh vector_e2e
```
