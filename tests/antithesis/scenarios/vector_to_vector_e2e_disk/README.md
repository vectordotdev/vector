# vector_to_vector_e2e_disk

The property under test is **conservation**: what goes in comes back out, no data
loss. Every event the loadgen-collector gets an ack for must eventually appear
back at the loadgen-collector, across arbitrary Antithesis faults. Duplicates are
allowed because the contract is at-least-once; a missing acked id is the bug.

## How it works

```
loadgen-collector --HTTP--> node0 --vector--> node1 --HTTP--> loadgen-collector
                            disk_v2
                            block+acks
```

Two Vector nodes and one loadgen-collector container.

- **node0** takes an `http_server` source and forwards over the native `vector`
  protocol to node1 through a `disk_v2` buffer with `when_full: block` and e2e
  acks. That buffer is the component under test.
- **node1** takes a `vector` source and delivers over `http` to the
  loadgen-collector. It has no disk buffer of its own.
- **loadgen-collector** is one container that injects unique event ids at node0
  and runs the HTTP collector that node1 delivers back to. With e2e acks, a 200
  to the injector means node0 durably accepted the event, and node0 holds it
  until node1 confirms the whole round trip. So an acked id that never comes back
  is lost.

Each container is its own pod so Antithesis can partition the links. node0 has a
persistent volume for its disk buffer, and the loadgen-collector has one for the
id logs, so a pod restart cannot erase the oracle's ground truth.

## Test commands

The compiled `harness` binaries are the test commands. The Dockerfile drops them
straight into the test template named by their Antithesis prefix. There are no
shell wrappers.

- `parallel_driver_produce` runs concurrent injectors at node0 and logs acked
  ids.
- `eventually_conservation` runs with faults paused. It drains both node buffers,
  requiring several consecutive empty-and-stable ticks, then asserts the buffers
  actually drained. A buffer that never drains is the #21683 deadlock. Then it
  asserts every acked id reached the collector, and that a fresh post-recovery
  write makes progress.

The `collector` binary is the loadgen-collector container's entrypoint, not a
test command. It records delivered ids and emits `setup_complete`.

## Run

```bash
cd tests/antithesis
docker compose -f scenarios/vector_to_vector_e2e_disk/docker-compose.yaml build
snouty validate scenarios/vector_to_vector_e2e_disk
```

See `../../scratchbook/properties/multi-hop-conservation-no-loss.md`.
