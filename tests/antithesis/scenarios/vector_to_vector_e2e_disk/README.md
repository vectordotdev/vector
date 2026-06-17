# vector_to_vector_e2e_disk

This scenario tests two properties.

**Conservation**: every event the oracle acked must eventually come back, across
arbitrary Antithesis faults. Duplicates are allowed because the contract is
at-least-once. A missing acked id is the bug.

**Liveness**: once faults stop, the writer must still accept fresh writes.

## How it works

Scenario has two Vector nodes and one oracle container.

- **head** takes an `http_server` source and forwards over the native `vector`
  protocol to tail through a `disk_v2` buffer with `when_full: block` and e2e
  acks. That buffer is the component under test.
- **tail** takes a `vector` source and delivers over `http` to the oracle through
  its own `disk_v2` buffer with the same `when_full: block` and e2e acks.
- **oracle** is one container that injects unique event ids at head and runs the
  HTTP endpoint tail delivers back to. A 200 from head's `http_server` means the
  event was end-to-end acked, but on the disk path that ack fires when the event
  is encoded into the buffer's in-memory write buffer, not when it is fsync'd to
  disk. Whether that ack survives faults is exactly the conservation property
  under test: an acked id that never comes back is lost data the client was told
  (200) was safe.

Each container is its own pod so Antithesis can partition the links. head has a
persistent volume for its disk buffer. The oracle keeps its id sets in memory and
Antithesis never terminates it, so the disk faults under test cannot corrupt the
judge.

## Run

```bash
cd tests/antithesis
docker compose -f scenarios/vector_to_vector_e2e_disk/docker-compose.yaml build
snouty validate scenarios/vector_to_vector_e2e_disk
```
