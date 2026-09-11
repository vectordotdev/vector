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
  `vector validate --no-environment` silently skipping VRL/condition errors by adding `validate_env()`
  and stub-enrichment plumbing. The resulting split between `validate()` (context-free, ignores its
  `&TransformContext` parameter) and `validate_env()` (VRL/condition compilation) is what this RFC
  cleans up.
- `TransformConfig` already has both hooks; this RFC renames them and formalizes the split by
  whether a check needs the context. `SinkConfig` has neither: it exposes only `build()`, which
  returns `(VectorSink, Healthcheck)`. What sinks lack is a structural phase that validates
  config-level construction without reaching the real endpoint.

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
  no VRL/condition compilation checks, so they do not need `validate_with_context`. Move the lexical
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
- Changes to user-visible configuration format.

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

No change to configuration format. The one observable difference is stricter
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

// Phase 2: construct the component. Side effects are allowed (resolve credentials, open
// clients). Must NOT spawn background tasks (task transforms excepted, see Scope):
// spawns move to startup so a rolled-back reload leaves nothing running.
//   Sinks:      build(context) -> (VectorSink, Healthcheck)   // as today
//   Transforms: build(context) -> Transform                   // as today

// Startup, unchanged.
//   Pre-commit:  topology builder runs Healthcheck (awaited if require_healthy; detached
//                otherwise). TopologyPiecesBuilder::build_transform() wires channels.
//   Post-commit: VectorSink::run() / spawn_diff() starts the Task.
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
`validate_structure` is owned by generic `ConfigBuilder` compilation; all paths inherit its result
transitively. `validate_with_context` runs in `vector validate` only: `build()` already compiles
the same VRL/conditions at startup, so running it again there would double-compile and could repeat
enrichment-index registration. Note: `validate_with_context` compiles against stub enrichment tables
(whose `add_index()` always succeeds) while `build()` uses the real ones (which can reject an
index), so they can disagree on enrichment-dependent errors; both must share the same compilation
helper.

`--skip-healthchecks` and the per-sink and global `healthcheck.enabled` gates and the configured
timeout remain the caller's responsibility (`TopologyPiecesBuilder`), exactly as today. `build()`
returns the raw probe future unchanged; nothing about healthcheck gating moves onto the component.

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


## Plan Of Attack

**Migration for transforms:**

1. Add `validate_structure(&self)` to `TransformConfig` with default `Ok(())`. Wire `validation.rs`
   to call it alongside `validate()` during the migration window. `validate_structure` is owned by
   `ConfigBuilder` compilation.
2. Rename `validate_env()` to `validate_with_context()` in a single PR: update the trait default,
   the one call site in `validate.rs`, and all implementors. Signatures are identical so no
   migration window is needed. `validate_with_context` is owned by `vector validate`, matching
   where `validate_env()` runs today.
3. Migrate `validate()` to `validate_structure()` one transform at a time. The signature drops
   `&TransformContext`, so this cannot be a straight rename. Transforms with real `validate()` logic
   need the full move-and-override treatment; all others only need `validate_structure()` added if
   they have structural checks hidden elsewhere (e.g. buried in `build()`). Open a tracking issue
   covering all affected transforms -- including those with no `validate()` but with pure checks in
   `build()` -- and check them off incrementally. Once all are done, remove `validate()` from the
   call sites and then from the trait.

**Migration for sinks:**

1. Add `validate_structure(&self)` to `SinkConfig` with default `Ok(())`. Wire `validation.rs` to
   call it. Migrate lexical checks out of `build()` one sink at a time (e.g. the
   [#25840](https://github.com/vectordotdev/vector/issues/25840) routing-field confinement check in
   `aws_s3`). Where a check is not a pure predicate (e.g. `Template::confine()` consumes its input
   and returns a protected copy), `validate_structure()` runs the check on a clone; `build()` still
   attaches the real runtime guard.
2. Enforce the no-spawn contract on sink `build()` per component. The audit must cover every task
   spawned directly or transitively from `SinkConfig::build()`. Examples: the GCP sinks
   (via `spawn_regenerate_token()`), `datadog_traces` (APM stats flush thread), and `redis`
   (connection repair task via `build_connection()`). Track the full list in a dedicated tracking
   issue. For each spawn: (a) bring the worker under the sink's `run()` future using structured
   concurrency (poll it in a `select!` or hold an abort-on-drop handle) so it stops when that
   future ends; and (b) decouple the healthcheck so the probe acquires and releases its own
   short-lived resources. Add regression tests for rollback and removal. The no-spawn contract may
   only be claimed trait-wide once every sink in the tracking issue has been migrated.

## Future Improvements

- Add `validate_structure()` to `SourceConfig`. Sources have no `build()`-returns-inert-handle shape
  today (`Source` is `BoxFuture<'static, Result<(), ()>>`, the run loop itself), so the pure-check
  phase is the natural first step there too: some sources (e.g. `socket`) already defer all work into
  the returned future; others (e.g. `file`) perform environment-dependent work eagerly and would
  need that separated from their structural checks.
- Rollback safety for task transforms: `TopologyPiecesBuilder::build_task_transform()` calls
  `t.transform(...)` pre-commit, and some task transforms spawn there (e.g. `throttle`). Deferring
  that invocation to post-commit is a separate change to the topology builder.
