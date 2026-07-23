# RFC - Separate Structural Validation from Component Build

Component config traits conflate two concerns in a single `build()` method: pure structural
validation (safe to run any time, including under `vector validate --no-environment`) and
side-effecting construction (resolving credentials, opening clients, building healthcheck probes).
This is most acute for `TransformConfig`, but the same shape of problem exists for `SinkConfig`.
This RFC introduces environment-free validation hooks alongside the existing `build()`: a shared
`validate_structure()` for context-free checks, plus (for transforms) a `validate_with_context()`
for checks that compile against the schema/enrichment context. `vector validate` can then check
configuration without touching the environment, and `build()` has a clear contract: it is the phase
where side effects are allowed.

## Context

- Immediate motivation: [#25161](https://github.com/vectordotdev/vector/pull/25161) fixed
  `vector validate --no-environment` silently skipping VRL/condition errors. It worked around the
  lack of a clean trait contract by adding `validate_env()` and the stub-enrichment plumbing that
  lets it run without a live environment (plus tests); the resulting split between `validate()` and
  `validate_env()` is what this RFC tidies.
- `TransformConfig` already carries two structural hooks that this RFC renames and cleans up:
  `validate()` (`src/config/transform.rs`) runs context-free structural checks (reserved output
  names, duplicate routes, sample rates), and `validate_env()` runs VRL/condition compilation only
  from `vector validate`. `validate()` takes a `&TransformContext` it largely ignores; at
  compilation it is handed a `TransformContext::default()` (`src/config/validation.rs`), and
  context-dependent checks are relegated to `validate_env()` rather than being guarded inline. The
  redundancy this RFC removes is therefore the unused context parameter on the structural hook and
  the two divergent call paths, not a large block of guard clauses. Splitting by whether a check
  needs the context (see Scope) lets the context-free hook drop its context parameter entirely.
- `SinkConfig` has neither hook: it exposes only `build()` (`src/config/sink.rs`). `build()` already
  returns `(VectorSink, Healthcheck)`, an unstarted sink plus a deferred environment probe, and the
  topology builder already treats `Healthcheck` as a distinct step (`run_healthchecks` in
  `src/topology/running.rs` awaits healthchecks before `spawn_diff` only when `require_healthy` is
  set; otherwise it detaches them, see the healthcheck note below).
  What sinks lack is the structural phase: a way to validate config-level construction
  without also reaching the real endpoint.

## Scope

### In scope

- For `TransformConfig`: rename the two existing hooks and split them by whether they need the
  context. `validate()` becomes `validate_structure(&self)` for context-free checks; it drops the
  `&TransformContext` parameter it currently ignores. `validate_env()` becomes
  `validate_with_context(&self, context)` for checks that compile against the assembled
  `TransformContext` (VRL programs, conditions, enrichment references), run in `vector validate`
  (both modes). At startup `build()` performs the equivalent compilation, so
  `validate_with_context` is not run there (avoiding a double compile; see the call-site note). This
  is a rename and clarifying split, not new surface; the signatures now enforce which checks may
  touch the context.
- For `SinkConfig`: add `validate_structure()` only (new; defaults to a no-op). Sinks have no
  schema-dependent structural checks, so they do not need `validate_with_context`. Move the lexical
  checks currently trapped in `build()` (e.g. the routing-field confinement check) into
  `validate_structure()` so `--no-environment` catches them.
- Establishing the contract that `build()` may have side effects but must not spawn background
  tasks; anything that spawns moves to startup (`run()` for sinks) so a rolled-back reload leaves
  nothing running. This contract is enforced for sinks and for migrated function/sync transforms.
  Task transforms are the known exception: `aws_ec2_metadata` spawns from `TransformConfig::build()`
  and others spawn from `TaskTransform::transform()`, both invoked pre-commit by
  `TopologyPiecesBuilder::build_task_transform()`. Enforcing the contract there requires a
  topology-builder change and is out of scope (see Motivation and Future Improvements). The contract
  is therefore stated as "must not spawn" with task transforms explicitly carved out until that
  change lands, rather than claimed as universally true on introduction.
- Healthchecks are side-effect-free probes, like Kubernetes readiness/liveness probes. A component's
  healthcheck must not share live background workers or mutable runtime state with the component; it
  may acquire its own short-lived resources (a token, a probe connection) and must release them
  promptly when it completes. Background workers are therefore owned by the sink inside its own
  `run()` future and terminate when that future is dropped, so there is no topology-owned lifecycle
  to manage even for a detached `require_healthy=false` probe. Existing sinks that violate this
  principle (by sharing a live worker or connection between their healthcheck and the sink) should be
  refactored to conform, altering their healthcheck behavior if necessary so it acquires and releases
  its own short-lived resources.
- Update `TopologyPiecesBuilder` and `vector validate` to call `validate_structure()` at the right
  point for both transforms and sinks.
- Migrate all existing transforms and sinks.

### Out of scope

- `SourceConfig` is deferred. `Source` is defined as `BoxFuture<'static, Result<(), ()>>`
  (`lib/vector-core/src/source.rs`), so `build()`'s return value *is* the run loop rather than an
  inert handle. Adding `validate_structure()` to sources is doable but a larger effort than this
  RFC's scope; see Future Improvements.
- Changes to user-visible configuration format or component behavior.

## Motivation

- `vector validate` has no clean way to "check VRL without starting threads." The current workaround
  (stub enrichment tables plus a separate `validate_env()` hook) must be replicated per-transform.
- `build()` spawns background tokio tasks before a topology reload is committed. If the reload is
  rolled back, those tasks leak. This RFC establishes that such spawns belong in startup, not
  `build()`, owned by the component future so they terminate when the component is removed. Sinks
  are clean here: their `build() -> run()` boundary is genuinely post-commit, so relocating a spawn
  such as `gcp_pubsub`'s token-regeneration task fixes the leak. Task transforms are not: their
  spawns (whether in `build()` like `aws_ec2_metadata`'s metadata-refresh loop, or in `transform()`
  like `throttle`'s rate-limiter flush) run pre-commit because `TopologyPiecesBuilder::build_task_transform()`
  invokes `transform()` before commit. Fixing task-transform rollback needs a topology-builder
  change (deferring `transform()` to post-commit) and is out of scope here.
- Testing transform logic requires spinning up background machinery because construction and startup
  are inseparable.
- For sinks, `--no-environment` is all-or-nothing: `vector validate` either skips `build()` entirely
  for every sink (no config validation beyond deserialization) or calls the real `build()`, which
  today also resolves real credentials and constructs a live `Healthcheck` future
  (e.g. `src/sinks/http/config.rs`, `src/sinks/kafka/config.rs`). There is no way to validate a
  sink's config-level construction (auth parsing, encoder setup, lexical checks) without also being
  able to reach the real endpoint.
- [#25840](https://github.com/vectordotdev/vector/issues/25840) is a concrete, currently-open
  instance of this gap. The routing-field template confinement check is purely lexical (no I/O), but
  it lives in `SinkConfig::build()` (e.g. `src/sinks/aws_s3/config.rs`), so `--no-environment` skips
  it and a confinement-violating config is only caught at real boot.

## Proposal

### User Experience

No change to configuration format or component behavior. The one observable difference is stricter
validation: `vector validate --no-environment` now runs `validate_structure` for sinks, so configs
with structural errors (e.g. the routing-field confinement check) that previously passed
`--no-environment` and only failed at boot will now be rejected earlier. This is a correctness
improvement, but it means a config that formerly passed `--no-environment` may now fail it.

### Implementation

The config-time phases are validation (context-free plus, for transforms, context-dependent checks)
and build, with a strict naming rule: a `validate_` method does no external I/O and spawns no tasks
(it may build derived in-memory state, e.g. enrichment indexes via `TableRegistry::add_index`), and
any method with external side effects is named for the construction it performs. Startup
(post-commit) is unchanged.

```text
// Phase 1: validation. No external I/O, no network, no spawning.
// (May build derived in-memory state such as enrichment indexes.)
// Split by whether a check needs the schema/enrichment context:

// 1a. Context-free checks: malformed URIs, out-of-range values, duplicate keys,
//     reserved output names. Runs everywhere, including generic config compilation.
//   Transforms + Sinks: validate_structure(&self) -> Result<(), Vec<String>>
//     (transforms: renamed from validate(); sinks: new, today sinks have no hook)

// 1b. Context-dependent checks: VRL/condition compilation against the assembled
//     context, whose enrichment tables may be stubs. Runs in `vector validate`
//     (both modes); at startup build() does the equivalent, so this is not
//     re-run there. Transforms only.
//   Transforms: validate_with_context(&self, context) -> Result<(), Vec<String>>
//     (renamed from validate_env())

// Phase 2: construct the component. Side effects are allowed (resolve
// credentials, open clients). Returns the built component and, for sinks, a
// deferred Healthcheck probe. Must NOT spawn background tasks (enforced for
// sinks and migrated function/sync transforms; task transforms are carved out,
// see Scope): anything that spawns moves to startup so a rolled-back reload
// leaves nothing running.
//   Sinks:      build(context) -> (VectorSink, Healthcheck)   // as today
//   Transforms: build(context) -> Transform                   // as today

// Startup, unchanged.
//   Pre-commit:
//     Sinks:      the topology builder handles the returned Healthcheck. With
//                 require_healthy=true it AWAITS it and gates commit on success;
//                 with require_healthy=false it DETACHES it (tokio::spawn) and
//                 proceeds, so the probe may finish after startup.
//     Transforms: TopologyPiecesBuilder::build_transform() wires channels and
//                 wraps the Transform into a Task.
//   Post-commit:
//     Sinks:      VectorSink::run().
//     Transforms: spawn_diff() starts the Task.
```

`build()` keeps its current signature for both traits, so no return-type reshaping is required. For
transforms this renames `validate()` to `validate_structure()` (dropping the `&TransformContext`
parameter it currently ignores) and `validate_env()` to `validate_with_context()`. For sinks it is
additive: a new `validate_structure()` in front of `build()`. In both cases `build()` gains the
discipline that it does not spawn, with task transforms carved out until the topology-builder change
lands (see Scope); this RFC does not deliver task-transform rollback safety.

Existing component wiring and serialization registration are unaffected.

**Call sites:**

| Call site | Transforms | Sinks |
| --- | --- | --- |
| `vector validate --no-environment` | `validate_structure` + `validate_with_context` | `validate_structure` |
| `vector validate` (full) | `validate_structure` + `validate_with_context` + `build` | `validate_structure` + `build`, then run the returned `Healthcheck` |
| Startup / reload (pre-commit) | `validate_structure` + `build` + `build_transform` (channel wiring) | `validate_structure` + `build`, then run or spawn the `Healthcheck` per `require_healthy` |
| Startup / reload (post-commit) | `spawn_diff` starts the Task | `VectorSink::run` |

The table shows which validation *applies* on each path, not a fresh invocation per row.
`validate_structure` (context-free) is executed once, owned by generic `ConfigBuilder` compilation;
because every path above compiles the `Config` first, they inherit its result rather than re-running
it. `validate_with_context` runs in `vector validate` (both modes, against a stub context). It is deliberately *not* run at startup: `build()` already compiles the same VRL programs
and conditions against the real context (e.g. `filter::build` calls `condition.build`), so running
`validate_with_context` there would double-compile and could repeat enrichment-index registration.
This matches today's behavior, where `validate_env()` is not called at startup. Note the two paths
are not identical: `validate_with_context` compiles against *stub* enrichment tables (whose
`add_index()` always succeeds) while `build()` uses the *real* tables (which can reject an index), so
they can still disagree on enrichment-dependent errors. To keep them from drifting, both must route
through the same compilation helper, and contextual (not just structural) parity between
`vector validate` and startup must be covered by tests (see Outstanding Questions).

`--skip-healthchecks` and the per-sink and global `healthcheck.enabled` gates and the configured
timeout remain the caller's responsibility (`TopologyPiecesBuilder`), exactly as today. `build()`
returns the raw probe future unchanged; nothing about healthcheck gating moves onto the component.

**Migration:**

1. Add both hooks with defaults that return `Ok(())`: `validate_structure(&self)` on both
   `SinkConfig` and `TransformConfig`, and `validate_with_context(&self, context)` on
   `TransformConfig`. No component changes are required to compile. Give each hook a single owner so
   there are no divergent call paths. `validate_structure` (context-free) is owned by generic
   `ConfigBuilder` compilation (`src/config/validation.rs`); `vector validate` and
   `TopologyPiecesBuilder` both consume an already-compiled `Config`, so they receive its result
   transitively and do not re-invoke it. `validate_with_context` is owned by `vector validate`
   (`src/validate.rs`), matching where `validate_env()` runs today; it is not run at startup because
   `build()` already compiles the same programs. Because `validate_structure` takes no context, the
   compilation-time call needs no context assembly, and every consumer keeps its structural coverage
   through compilation.
2. Migrate transforms one at a time: rename `validate()` to `validate_structure()` (dropping its
   unused context parameter) and `validate_env()` to
   `validate_with_context()`, splitting any remaining context-dependent logic out of the former into
   the latter. Start with `remap` (VRL) and `filter` / `route` (conditions). Prerequisite for
   `remap`: move VRL file reading (`file:`/`files:` options) to config load time so
   `compile_vrl_program` never does file I/O. Until a transform is migrated, `vector validate`
   continues calling its legacy `validate()`/`validate_env()` so no check is silently dropped during
   the migration window.
3. Migrate sinks one at a time: add the sink-side lexical checks (e.g. the #25840 routing-field
   confinement check in `aws_s3`) to `validate_structure()` so `--no-environment` catches them. Note
   that `Template::confine()` is not a pure check: it consumes the template and returns a protected
   copy with a runtime escape checker attached, which `build()` then passes to the partitioner
   (`src/template.rs`, `src/sinks/aws_s3/config.rs`). `validate_structure()` therefore performs a
   preflight confinement check on a clone; `build()` must still attach confinement to the runtime
   template. The security boundary stays in `build()`; `validate_structure()` only surfaces the
   error earlier.
4. Enforce the no-spawn contract on sink `build()` per component. The audit must cover every task
   spawned directly or transitively from `SinkConfig::build()`, not just one helper. Known examples
   span more than the GCP sinks:
   - `spawn_regenerate_token()`, called from `build()` in at least `gcp_pubsub`, `gcp_cloud_storage`,
     `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, and `gcp_chronicle` (`src/sinks/gcp/*`,
     `src/sinks/gcp_chronicle/*`). It detaches the task and returns only a `watch::Receiver`, and its
     credential loop never observes closure (`src/gcp.rs`), so the task outlives a dropped sink.
   - `datadog_traces`, which starts the APM stats flush thread during `build()`
     (`src/sinks/datadog/traces/config.rs`).
   - `redis`, whose connection repair task is spawned transitively via `build_connection()`
     (`src/sinks/redis/sink.rs`, `src/sinks/redis/config.rs`); it is abort-on-drop today but still
     violates the strict no-spawn contract.
   For each spawn: (a) bring the worker under the sink's `run()` future using structured concurrency
   so it actually stops when that future ends. A bare `tokio::spawn` inside `run()` stays detached
   and would preserve the leak; instead poll the worker as a child future (e.g. in a `select!`) or
   hold an abort-on-drop handle that is aborted/awaited during shutdown. And (b) decouple the
   healthcheck from that worker, giving the healthcheck its own short-lived resources rather than a
   clone of the sink's live connection or refresher (see the side-effect-free healthcheck contract in
   Scope). Because the healthcheck no longer shares the worker, a detached `require_healthy=false`
   probe can outlive `run()` harmlessly, and there is no topology-owned lifecycle to manage. Add
   regression tests for both rollback and removal proving the worker stops when the sink is dropped,
   including the detached-healthcheck case. (Task-transform spawns such as `aws_ec2_metadata`'s are excluded; see
   Motivation.) The no-spawn contract may only be claimed for sinks once the tracking issue's sink
   audit is complete: every sink that spawns directly or transitively in `build()` has been relocated
   into `run()` and decoupled from its healthcheck, with a rollback/removal test. Until then it is a
   per-migrated-sink property, not a trait-wide guarantee.
5. Remove `validate()` and `validate_env()` only after every transform has been migrated off them;
   until then the default no-op `validate_structure()` on un-migrated transforms must not replace the
   legacy checks. This is a blocking prerequisite, tracked per the Plan of Attack below.

## Alternatives

- **Keep the current approach and add more per-transform workarounds.** Already proven insufficient:
  the fix PR added hundreds of lines of guard logic with no improvement to the trait contract.
- **Add a separate `validate_environment` phase between `validate_structure` and `build`.** An
  earlier draft of this RFC did exactly that. It was rejected for two reasons. First, naming: a
  `validate_` method that resolves credentials, opens connections, and spawns a token-refresh task
  is not validating, it is constructing; the name would lie about its side effects. Second, a
  separate phase adds machinery for no gain: `build()` already returns a self-contained `Healthcheck`
  future that the topology builder runs, and under the side-effect-free healthcheck contract that
  future owns its own short-lived probe resources anyway. Keeping healthcheck construction inside
  `build()`, exactly as the `(VectorSink, Healthcheck)` shape does today, needs no state-transfer
  channel and no extra phase. The genuine environment check, the healthcheck probe, stays a deferred
  future the topology builder runs; running it is the validation.
- **A single `validate_structure(context)` for transforms instead of the two-hook split.** This
  would require the assembled `TransformContext` (stub enrichment plus merged schema) at every call
  site, including generic `ConfigBuilder` compilation, which today only has
  `TransformContext::default()`. Building the merged schema there needs the input graph and is a
  substantial change. The rejected fallback, passing a partial/default context and guarding on
  `context.key.is_some()`, is the guard pattern the current `validate()`/`validate_env()` split was
  designed to avoid; folding both into one context-carrying method would reintroduce it. The chosen
  split keeps context-free checks context-free (no assembly, no guards) and confines the context
  requirement to `validate_with_context`, which only runs where the context already exists.

## Outstanding Questions

- Regression coverage: add tests showing that generic config compilation, `vector validate
  --no-environment`, and startup report equivalent errors, so the split between `validate_structure`
  and `validate_with_context` cannot silently drop a check on one path. Parity is defined narrowly, to
  the checks that do not depend on real environment state: VRL/condition syntax, types, and
  table-name resolution. It cannot extend to errors that only a real table produces:
  `validate_with_context` compiles against stub enrichment tables (whose `add_index()` always
  succeeds) while startup `build()` uses real ones (which can reject an index), so those
  real-table-only errors are tested separately as surfacing during full validation/startup, not as
  parity with `--no-environment`. Both paths should still share one compilation helper so the
  parity-scoped checks cannot drift.
- Task-transform rollback safety (deferring `TaskTransform::transform()` to post-commit) is left to
  a follow-up; see Future Improvements. Should it block removing the migration's task-transform
  carve-out, or ship independently?

## Plan Of Attack

1. Add `validate_structure()` (both traits) and `validate_with_context()` (transforms) with no-op
   defaults, and wire them into generic compilation, `vector validate`, and `TopologyPiecesBuilder`.
   Prove the pattern with one transform (`remap`) and one sink (moving the `aws_s3` confinement
   check).
2. Open a tracking issue listing every transform still on `validate()`/`validate_env()` and every
   sink with lexical checks or with a direct/transitive spawn inside `build()`; migrate them
   incrementally, checking each off.
3. Complete the sink no-spawn audit as a blocking gate before declaring the RFC implemented: every
   sink that spawns directly or transitively from `build()` (see migration step 4) has been
   relocated into `run()` with structured ownership and a rollback/removal test. Until this gate is
   met, the no-spawn contract is a per-migrated-sink property, not a trait-wide guarantee.
4. Remove `validate()` and `validate_env()` only after every transform on the tracking issue is
   migrated. This is a blocking prerequisite: while any transform still relies on the legacy checks,
   the no-op default `validate_structure()` would silently drop its VRL/condition validation.

## Future Improvements

- Add `validate_structure()` to `SourceConfig`. Sources have no `build()`-returns-inert-handle shape
  today (`Source` is `BoxFuture<'static, Result<(), ()>>`, the run loop itself), so the pure-check
  phase is the natural first step there too: some sources (e.g. `socket`) already defer all work into
  the returned future; others (e.g. `file`) perform environment-dependent work eagerly and would
  need that separated from their structural checks.
- Rollback safety for task transforms: `TopologyPiecesBuilder::build_task_transform()` calls
  `t.transform(...)` pre-commit, and some task transforms spawn there (e.g. `throttle`). Deferring
  that invocation to post-commit is a separate change to the topology builder.
