This directory contains files for running Antithesis tests against Vector's
**disk buffer v2** (`lib/vector-buffers/src/variants/disk_v2/`).

Use the `antithesis-research` skill to analyze the system and build the property
catalog (see `scratchbook/`). Use the `antithesis-setup` skill to scaffold this
directory. Use the `antithesis-workload` skill to implement assertions and test
commands. Use the `antithesis-launch` skill to build, validate, and submit runs
— do not run `snouty launch` directly.

## Topology (see scratchbook/deployment-topology.md)

Two containers, single network:

- `vdbuf-vector` (SUT): minimal-feature Vector. `http_server` source (e2e acks)
  -> `http` sink with a **disk buffer** -> workload collector;
  `internal_metrics` -> `prometheus_exporter` on :9598. Disk buffer `data_dir`
  is on the persistent named volume `vdbuf-buffer` so it survives node-kill.
- `vdbuf-workload` (client): instrumented Rust driver (`antithesis/workload/`).
  Entrypoint `serve` runs the sink collector, waits for Vector, emits
  `setup_complete`, and produces uniquely-IDed events. Test template at
  `/opt/antithesis/test/v1/`.

## Instrumentation status

- **Workload**: instrumented (LLVM sancov via `antithesis-instrumentation`) +
  `antithesis_sdk` assertions; unstripped binary symlinked into `/symbols`.
- **Vector (Phase 1)**: built minimal-feature, release, **uninstrumented** (no
  sancov, no `antithesis_sdk`) to keep the first build fast/low-risk. First
  failure demos are workload-observable. SUT-side instrumentation is added in
  the grind phase for the deadlock/underflow properties
  (`total-buffer-size-never-underflows`, `writer-eventually-makes-progress`).

## snouty

- **launch**: `snouty launch --json --webhook basic_test --config tests/antithesis/config`
  (only the `basic_test` webhook is available for now). Run `docker compose build`
  first. Requires `ANTITHESIS_API_KEY` (sourced from `~/.config/snouty/secrets.env`).
- **validate**: `snouty validate tests/antithesis/config`

## Local smoke test

```sh
docker compose -f tests/antithesis/config/docker-compose.yaml build
docker compose -f tests/antithesis/config/docker-compose.yaml up
# expect: vector healthy, workload emits setup_complete + reachable assertions
```

## Subdirectories

- `config/`   — `docker-compose.yaml` + `vector.yaml`
- `workload/` — Rust workload driver crate
- `test/v1/`  — Antithesis test template (test command executables)
- `scratchbook/` — research artifacts (property catalog, SUT analysis, etc.)
- `Dockerfile` — multi-stage build (`vector`, `workload` targets)
- `setup-complete.sh` — emits the `setup_complete` lifecycle event (also emitted
  by the workload binary)
