# Antithesis: disk buffer v2

An Antithesis test for Vector's disk buffer v2, which lives at
`lib/vector-buffers/src/variants/disk_v2/`. Everything needed to run it is in this
directory.

The experiment is **`vector_to_vector_e2e_disk`**. node0 takes an http source and
forwards to node1 over the native `vector` protocol through a `disk_v2` buffer
with `when_full: block` and e2e acks, and node1 delivers back over http. The
property is **conservation**: every event the loadgen-collector gets an ack for
must eventually come back out. What goes in comes back out, no data loss, across
Antithesis container kills, restarts, and network partitions.

## Quick start

```bash
cd tests/antithesis
docker compose -f scenarios/vector_to_vector_e2e_disk/docker-compose.yaml build
snouty validate scenarios/vector_to_vector_e2e_disk
```

See AGENTS.md for the layout and conventions,
`scenarios/vector_to_vector_e2e_disk/README.md` for the experiment in detail, and
`scratchbook/` for the system analysis and property catalog.

## Prerequisites

- Docker or Podman
- snouty, the Antithesis CLI, from https://github.com/antithesishq/snouty
- To launch real runs: the antithesis-launch skill and tenant credentials.
