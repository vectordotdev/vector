# RFC 2026-06-09 - Parse-First Config Interpolation

Parse the config document into a native value tree first, apply `${VAR}` and `SECRET[...]`
substitution only to string leaves, and run a JSON-Schema-driven coercion pass to convert
string scalars to declared types before serde runs.

## Context

- Secret management design: [RFC 11552](2022-02-24-11552-dd-agent-style-secret-management.md)
- [#23910](https://github.com/vectordotdev/vector/pull/23910) — added `--disable-env-var-interpolation`
- [#24088](https://github.com/vectordotdev/vector/pull/24088) — prevented multiline env var interpolation
- [#21282](https://github.com/vectordotdev/vector/pull/21282) — file/directory secret backends

## Cross cutting concerns

- `vector-config` schema generation (`generate_root_schema::<ConfigBuilder>()`) must remain
  accurate for the coercion pass to be correct.
- All secret backends implement `SecretBackend::retrieve(...) -> HashMap<String, String>`;
  any future backend must continue to honour this contract.

## Scope

### In scope

- Moving env-var and secret substitution from raw-text regex passes to tree-walks over
  `String` leaves in the parsed value tree.
- Moving secret placeholder collection to the same tree-walk, replacing the raw-text scan.
- Adding a validate/coerce pass that uses Vector's JSON Schema to coerce string scalars to
  declared types and warn on unknown fields with their full path.
- Migration guidance for users relying on pre-parse substitution behaviour.

### Out of scope

- Emitting `#[serde(alias)]` entries into the generated JSON Schema (tracked TODO in
  `vector-config/src/lib.rs`). Until that is done, unknown-field detection remains a warning
  rather than a hard error.
- Adding a `--disable-secret-interpolation` CLI flag (natural follow-up once the tree-walk is
  in place).
- Migrating the HTTP config provider to the new pipeline (separate follow-up).

## Motivation

Vector's config loading pipeline has two deeply entangled problems that together make this one of
the hardest areas of the codebase to work on.

### 1. User-facing configuration errors

When a Vector config is invalid, serde reports a type error or an unknown-field error with no
indication of where in the config the problem is. Users routinely open GitHub issues like "what
does this error mean?" with a serde message that names neither the field nor the file:

```text
error: unknown field `retries`, expected one of `encoding`, `batch`, ...
```

There is no field path like `sinks.my_sink.retries`. This is a known pain point with open issues
that cannot be cleanly fixed under the current architecture, because the config is fully
deserialized in one shot by serde before any path context is available.

### 2. Hard to maintain code

Vector performs string substitution on the raw config text before TOML/YAML/JSON deserializers
run. Both environment variable interpolation (`${VAR}`) and secret substitution
(`SECRET[backend.key]`) operate at this layer.

This creates compounding tech debt:

- **Secret backends cannot inspect or transform typed values.** They receive and return raw
  strings. Any future work on secrets becomes awkward, including richer types, per-field
  policies, audit trails, and conditional substitution, because substitution happens before
  the document is a tree.

- **Format-specific behavior is impossible to reason about.** Interpolation happens before the
  format is fully parsed, so whether `${VAR}` appears inside a TOML string, a TOML integer, a YAML
  block scalar, or a JSON key is invisible to the interpolation code. Edge-case bugs are hard to
  reproduce and harder to fix without risking regressions elsewhere.

- **Testing this code is painful.** Because substitution and parsing are entangled, unit tests have
  to construct raw config strings rather than value trees. Adding new features requires
  understanding the interaction between two layers that should be independent.

- **Type coercion is implicit and fragile.** When a placeholder appears in a non-string position
  (`port = SECRET[db.port]`), the substituted text must be syntactically valid TOML/JSON at that
  position. This works only because substitution runs before the format parser sees the document.
  The constraint is undocumented, not tested systematically, and breaks in ways that are hard to
  diagnose.

- **The `--disable-env-var-interpolation` flag has no clean extension point.** The flag itself
  is a useful and intentional control, but the raw-text pipeline gives it nowhere to grow.
  An equivalent `--disable-secret-interpolation` flag, for example, would require threading a
  new boolean through the loading stack and adding another regex pass over raw text. Under the
  new pipeline it becomes a flag that skips the secret tree-walk, with no raw string handling
  needed.

These pain points have made contributors hesitant to touch this code. Non-trivial improvements
stall because they require untangling parsing and substitution first.

## Proposal

### User Experience

**Better error messages.** Today:

```text
x unknown field `retries`, expected one of `print_interval_secs`, `rate`, `acknowledgements`

  in `sinks.my_sink`
```

After this change:

```text
x unknown `field` at sinks.my_sink.retries
```

```text
x expected integer at sources.my_source.count, found string "not-a-number"
```

Users get a single, clean error with the full field path.

**Spec-compliant configs.** Today, `count = ${MY_COUNT}` works in Vector but is not valid TOML.
After this change, configs are real TOML, JSON, and YAML, so off-the-shelf editors and linters
work correctly against them.

**Migration.** The common case is mechanical: wrap unquoted placeholders in quotes
(`count = "${MY_COUNT}"`, `port = "SECRET[db.port]"`) and the coercion pass converts the value
to the declared type.

Structural uses of interpolation are not valid TOML or JSON and are not supported in the new
model. This includes table headers (`[${SECTION}]`), map keys (`${KEY} = ...`), and inline
arrays (`inputs = [${VECTOR_INPUTS}]`). These patterns are rare in practice and need to be
replaced with literal values. The implementation will follow Vector's deprecation policy to give
users time to migrate.

A second silent-change case applies to YAML keys and TOML quoted keys: `${KEY}: value` is valid
YAML (and `"${KEY}" = value` is valid TOML), so neither produces a parse error. Since keys are
never walked by the interpolation pass, the key remains the literal string `${KEY}` rather than
the substituted value. The implementation will detect placeholders in key position and warn,
directing users to replace them with literal key names.

One YAML-specific case requires attention: `inputs: [${VECTOR_INPUTS}]` is valid YAML syntax
(an array containing one string element), so it does not produce a parse error under the new
model. However, the behavior changes silently — today the raw-text substitution of
`source_a, source_b` produces a two-element array; after this change it produces a one-element
array containing the literal string `"source_a, source_b"`. Users relying on this pattern must
replace it with explicit literal values.

### Implementation

#### Pipeline

```text
raw text
  -> parse
  -> value tree
  -> interpolate env vars
  -> resolve secrets (coerce secret subtree, fetch, substitute)
  -> coerce full tree
  -> deserialize
```

Both `${VAR}` and `SECRET[backend.key]` placeholders use the same string-leaf substitution
mechanism, applied in separate stages. Objects and arrays are walked recursively until string
leaves are reached; integers, booleans, and keys are never touched:

```text
          value tree                        after interpolation

          sources                               sources
             |                                    |
          my_source --------------------------> my_source
           +-- type: String("demo_logs")  ---->  +-- type: String("demo_logs")   (unchanged)
           +-- count: Integer(100)        ---->  +-- count: Integer(100)         (unchanged)
           +-- endpoint: String("${URL}") ---->  +-- endpoint: String("https://...") <- substituted
                              ^
                              |
                         only String leaves
                         are walked by the
                         interpolation pass
```

#### Files

1. **`interpolation.rs`**: env-var and secret regex applied to `toml::Value::String` leaves only.
2. **`schema_coercion.rs`**: recursive JSON Schema walker that coerces scalar types. Runs twice:
   once as part of secret resolution (secret backend configs may use env var placeholders), and
   once on the full tree after secret substitution.
3. **`loader.rs`**: `Process::load()` default: parse, interpolate env vars, run `postprocess`
   for secret substitution, then validate and coerce. All steps operate on the parsed tree.
   Secret placeholder collection moves to a tree-walk over string leaves as well, replacing the
   current raw-text regex scan. This means `SECRET[...]` in YAML comments, map keys, or other
   non-string positions is no longer collected or sent to the backend.
4. Public API signatures unchanged (`load_from_paths`, etc.).

## Design Decisions

### Secret backends always return strings

The `SecretBackend` trait is defined as `retrieve(...) -> HashMap<String, String>`. Every backend
(exec, file, directory, AWS Secrets Manager) returns string values regardless of the field type
the secret will populate. This is an explicit design choice, modeled on Datadog Agent secret
management behavior ([RFC 11552](2022-02-24-11552-dd-agent-style-secret-management.md)). The
coercion pass is therefore the only place where a secret value destined for a numeric or boolean
field is converted to the declared type.

### Coercion failures are hard errors

The alternative is a soft warning that falls back to passing the raw string to serde, but that is
not meaningfully safer. `serde_json` does not perform implicit string-to-number or string-to-bool
coercion, so a `u64` field receiving `Value::String("42")` will fail at the serde layer. There is
no known class of configs that currently works and would be broken by a hard error in the new
pass. Failing early with a field-path-aware error is strictly better than surfacing a confusing
serde error or silently loading a misconfigured value.

### Unknown-field detection is currently non-fatal

`vector-config` does not yet emit `#[serde(alias = "...")]` aliases into the generated JSON
Schema (tracked TODO in `vector-config/src/lib.rs`). Many Vector fields carry user-facing aliases
(`host`, `token`, `namespace`, `url`, and others), so a key absent from the schema may still be a
valid alias that serde accepts. The coerce pass collects unknown-field paths at debug level and
defers the authoritative check to serde. If serde subsequently errors on that field, Vector
surfaces the path-aware message from the coerce pass instead of the raw serde output — giving
users one clean error. When aliases are emitted in the schema, the coerce pass can hard-error
directly without waiting for serde.

### Unknown-field checking is skipped for unrecognized component types

When a component's `type` value does not match any compiled variant (for example, the component
is feature-gated and not built), the check is suppressed. The component's fields are not in the
schema and cannot be validated. Suppressing avoids false positives and preserves the existing
behavior of passing unrecognized components through to serde, which produces the "unknown variant"
error with its own context.

## Rationale

- Field-path-aware errors directly address a category of open issues where users cannot diagnose
  their own config errors without asking in community channels.
- Interpolation and parsing become independently testable.
- Secret backend extensibility (richer types, per-field policies) no longer requires touching
  the parser layer.
- Substituted values that contain structural characters (newlines, quotes, braces) remain string
  scalars and cannot grow new config keys or sections, eliminating a config injection surface.

## Drawbacks

The validate/coerce pass adds implementation complexity upfront. It uses Vector's own JSON
Schema (`generate_root_schema::<ConfigBuilder>()`) to infer expected types and detect unknown
fields, which requires handling discriminated-union types (`oneOf` with a `type` tag) and
`additionalProperties`-style open maps correctly.

This is a one-off cost. Once the pass is in place it tracks the generated schema automatically,
and the config loading pipeline becomes easier to extend and test going forward.

## Prior Art

The Datadog Agent uses the same exec-based secret backend model with string-only return values
([RFC 11552](2022-02-24-11552-dd-agent-style-secret-management.md)). Vector's `SecretBackend`
trait was designed to be compatible with this protocol.

## Alternatives

### Option A: `CoercingDeserializer` wrapping serde

A custom `Deserializer` implementation that intercepts `visit_str` calls and coerces the value to
the type the downstream `Visitor` requests. This keeps all type information inside serde and
avoids a separate schema-walking pass. The approach works for simple structs but breaks for
internally-tagged enums (`#[serde(tag = "type")]`): serde buffers the entire map into an opaque
`Content` value before dispatching to the variant, so the custom deserializer never sees the
nested field types and cannot coerce them. Vector's component configs are pervasively tagged
enums, so this option does not generalize.

### Option B: Run a JSON Schema validator instead of writing a custom pass

Run an off-the-shelf JSON Schema validator (for example, the `jsonschema` crate) over the value
tree before deserialization and surface its errors. Validators report structural conformance well
and already produce path-aware errors, but they do not perform coercion: a `"42"` string in an
integer field would hard-fail instead of being converted. Coercing env-var-derived strings to
their declared scalar types is the whole point of the new pass, so a validator alone is not a
substitute. It also cannot account for Vector's custom serde logic (untagged unions, aliases)
that intentionally diverges from the raw JSON Schema.

### Option C: `serde_path_to_error` (or equivalent) on top of serde

The `serde_path_to_error` crate annotates serde errors with the field path at which they
occurred, which would solve the "no file/field path" half of the motivation without a
schema-walking pass. It does not solve coercion (string-to-int from env vars still fails at
serde), so it is at best a partial fix. It also inherits the same `Content`-buffering limitation
as Option A on internally-tagged enums: once serde buffers the map for variant dispatch, the path
information is lost for nested fields. Vector's tagged-enum-heavy schema is exactly where
path-aware errors are most needed, so this approach degrades in the cases that matter most.

## Outstanding Questions

None blocking merge.

## Plan Of Attack

- [ ] Announce deprecation of structural interpolation patterns and the pre-parse substitution behaviour.
- [ ] Wait for the deprecation period to elapse.
- [ ] Implementation PR: parse-first pipeline, coercion pass, tree-walk secret collection.
- [ ] Release.

## Future Improvements

- Emit `#[serde(alias)]` entries into the generated JSON Schema, enabling unknown-field
  detection to be promoted from a warning to a hard error.
- Add `--disable-secret-interpolation` CLI flag as a tree-walk filter.
- Migrate the HTTP config provider to the parse-first pipeline.
- Replace `toml::Value` with `serde_json::Value` as the internal value tree type
  ([#19963](https://github.com/vectordotdev/vector/issues/19963)). The parse-first pipeline makes
  this a bounded interface change: the value tree is the canonical intermediate representation, so
  the node type can be swapped without touching raw-text handling. This addresses the long-standing
  lack of a null type in `toml::Value`.
