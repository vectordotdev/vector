# Antithesis test for Vector's disk buffer v2

Everything for running this test under Antithesis lives in this directory. The
only Antithesis touch-point elsewhere in the repo is the feature-gated
`antithesis` force-link in `lib/vector-buffers`, which makes the SUT link the
sancov coverage runtime.

## The experiment

There is one: **`vector_to_vector_e2e_disk`**. Two Vector nodes and one
loadgen-collector container:

```
loadgen-collector --HTTP--> node0 --vector--> node1 --HTTP--> loadgen-collector
                            disk_v2
                            block+acks
```

node0 takes an `http_server` source and forwards to node1 over the native
`vector` protocol through a `disk_v2` buffer with `when_full: block` and e2e
acks. That buffer is the component under test. node1 takes a `vector` source and
delivers over `http` back to the loadgen-collector. The property is
**conservation**: what goes in comes back out, no data loss, across Antithesis
container kills, restarts, and network partitions. One loadgen-collector
container injects unique ids at node0 and collects what comes back from node1.

## Layout

```
tests/antithesis/
├── Dockerfile        One multi-target image with a vector target and a workload
│                     target. Build context is the repo root. Each build asserts
│                     the instrumentation symbols are present, the saluki safety
│                     net.
├── harness/          The Rust crate. It is its own workspace and HTTP-only, so it
│                     does not perturb Vector's build. src/lib.rs is the shared
│                     logic; each src/bin/<prefix>_<name>.rs is a test command or
│                     the collector entrypoint.
├── scenarios/
│   └── vector_to_vector_e2e_disk/   docker-compose.yaml and node0/1.yaml. snouty
│                                     consumes this dir as --config.
└── scratchbook/      Durable Antithesis research: sut-analysis, property-catalog,
                      properties/, deployment-topology, evaluation/.
```

## Test commands, in `harness/src/bin/`

The compiled binaries are the test commands. The Dockerfile drops them straight
into the test template named by their Antithesis prefix; there are no shell
wrappers.

- `parallel_driver_produce` injects uniquely-IDed events at node0 and logs acked
  ids.
- `eventually_conservation` drains both node buffers, then asserts the buffers
  drained, that every acked id reached the collector, and that a post-recovery
  write makes progress.

`collector` is the long-lived loadgen-collector entrypoint, not a test command.
It is the HTTP sink node1 delivers to; it records delivered ids and emits
`setup_complete`.

## Commands

```bash
cd tests/antithesis
docker compose -f scenarios/vector_to_vector_e2e_disk/docker-compose.yaml build
snouty validate scenarios/vector_to_vector_e2e_disk
```

Use the antithesis-launch skill to submit a real run; do not run `snouty launch`
directly. The harness crate is built and checked from its own dir as a separate
workspace: `cd tests/antithesis/harness && cargo build --bins`.

## Conventions

- A test command is `harness/src/bin/<prefix>_<name>.rs`. Valid prefixes:
  `parallel_driver_ singleton_driver_ serial_driver_ first_ eventually_
  finally_ anytime_`. The collector emits `setup_complete` via the SDK once the
  SUT is up.
- The SUT and harness builds set the `antithesis` cargo feature to force-link the
  sancov runtime, and apply the sancov RUSTFLAGS to the target only via
  `--config`. The Dockerfile then nm-checks the symbols are present.
- Keep the scratchbook current as Antithesis decisions change.

## Agent behaviour

- The human is primary. If you hit confusion, pause and ask.
- Truth over comfort. Report what is actually true, including a harness that does
  not build, a config that is a duplicate, or a documented guarantee that is not
  real.
- Code is liability. Delete what does not earn its place, and do not dribble test
  artifacts across the program.
