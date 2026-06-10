# RFC 2026-06-09 - Parse-First Config Interpolation

## Motivation

Vector's config loading pipeline has two deeply entangled problems that together make this one of
the hardest areas of the codebase to work on.

### 1. User friendly configuration errors

When a Vector config is invalid, serde reports a type error or an unknown-field error with no
indication of where in the config the problem is. Users routinely open GitHub issues like "what
does this error mean?" with a serde message that names neither the field nor the file:

```text
error: unknown field `retries`, expected one of `encoding`, `batch`, ...
```

There is no file path, no line number, no field path like `sinks.my_sink.retries`. This is a
known pain point with open issues that cannot be cleanly fixed under the current architecture,
because the config is fully deserialized in one shot by serde before any path context is
available.

Producing actionable, field-path-aware errors for unknown fields and type mismatches is a
first-class goal of this RFC.

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
  Extending the same idea to secrets, or to field-level suppression, would require rethinking
  the pipeline first.

These pain points have made contributors hesitant to touch this code. Non-trivial improvements
stall because they require untangling parsing and substitution first.

## Proposed Solution

Parse the config document into a native value tree first, then apply substitution only to
`String`-typed leaf nodes. Add a structured validation/coercion pass between substitution and
final deserialization.

```text
raw text -> parse -> value tree -> interpolate -> validate/coerce -> deserialize
```

Substitution operates on `toml::Value::String` leaves only. Structural nodes (objects, arrays,
integers, booleans, keys) are never touched:

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

The validate/coerce pass walks the tree using Vector's own JSON Schema
(`generate_root_schema::<ConfigBuilder>()`) and:

- Coerces `"42"` to `42`, `"true"` to `true` where the schema declares a non-string type
- Detects unknown fields and reports them with their full path
- Detects type mismatches and reports them with their full path and the expected type

This gives each stage a clean contract:

- The parser sees raw text, unmodified.
- The interpolation layer sees a typed tree and operates only on strings.
- The validation pass has the full field path and can produce actionable errors.
- Downstream deserialization receives a well-typed tree that will not produce serde errors.

## Benefits

### User-facing error quality

Today:

```text
error: unknown field `retries`, expected one of `encoding`, `batch`, `request`, ...
```

After this change:

```text
error: unknown field at sinks.my_sink.retries
```

```text
error: expected integer at sources.my_source.count, found string "not-a-number"
```

Users get a field path. The path includes the component name, making it immediately actionable.
This directly addresses a category of open issues where users cannot diagnose their own config
errors without asking in community channels.

### Improved developer experience

- Interpolation and parsing become independently testable.
- Secret backend extensibility (richer types, per-field policies) no longer requires touching
  the parser layer.
- The `--disable-env-var-interpolation` flag has a clean extension point: skip the tree-walk,
  rather than stripping regex matches from raw text.

### Spec-compliant configs

Today, a config like `count = ${MY_COUNT}` works in Vector but is not valid TOML. The TOML and
JSON specifications have no variable-substitution syntax, so configs that rely on the pre-parse
shortcut cannot be linted, formatted, or syntax-highlighted by any off-the-shelf TOML/JSON tool.
After this change, Vector configs are real TOML, JSON, and YAML, so the broader ecosystem of
editors and linters works correctly against them. The migration is mechanical: wrap the
placeholder in quotes (`count = "${MY_COUNT}"`) and let the new coercion pass convert the
string to the declared type.

### Migration

The common case is mechanical: wrap unquoted placeholders in string positions in quotes
(`count = "${MY_COUNT}"`) and the coercion pass converts the value to the declared type.

Structural uses of interpolation — table headers (`[${SECTION}]`), map keys (`${KEY} = ...`),
or unquoted booleans/arrays — are not valid TOML or JSON and are not supported in the new model.
These patterns are rare in practice; configs that use them need to be restructured.

### Improved security

Substituted values that contain structural characters (newlines, quotes, braces) remain string
scalars. They cannot grow new config keys or sections. This eliminates a config injection surface
that is difficult to close under the current architecture.

## Trade-offs

The **validate/coerce pass** adds implementation complexity upfront. It uses Vector's own JSON
Schema (`generate_root_schema::<ConfigBuilder>()`) to infer expected types and detect unknown
fields, which requires handling discriminated-union types (`oneOf` with a `type` tag) and
`additionalProperties`-style open maps correctly.

This is a one-off cost. Once the pass is in place it tracks the generated schema automatically,
and the config loading pipeline becomes easier to extend and test going forward.

## Implementation Sketch

1. **`interpolation.rs`**: env-var and secret regex applied to `toml::Value::String` leaves only.
2. **`schema_coercion.rs`**: recursive JSON Schema walker that coerces scalar types. Unknown-field
   detection and path-aware error reporting are implemented via `unevaluatedProperties: false`
   handling on Vector's outer wrapper schemas (`SourceOuter`, `SinkOuter`, `EnrichmentTableOuter`,
   etc.), which have visibility into the full union of valid properties for a component including
   shared fields (`inputs`, `proxy`, `graph`).
3. **`loader.rs`**: `Process::load()` default: parse, interpolate env vars, run `postprocess`
   for secret substitution, then validate and coerce. All steps operate on the parsed tree.
4. Public API signatures unchanged (`load_from_paths`, etc.).

## Design Decisions

**Coercion failures are hard errors.** The alternative is a soft warning that falls back to
passing the raw string to serde, but that is not meaningfully safer. `serde_json` does not
perform implicit string-to-number or string-to-bool coercion, so a `u64` field receiving
`Value::String("42")` will fail at the serde layer. There is no known class of configs that
currently works and would be broken by a hard error in the new pass. Failing early with a
field-path-aware error is strictly better than surfacing a confusing serde error or silently
loading a misconfigured value.

**Unknown-field detection uses `unevaluatedProperties: false`, not `additionalProperties: false`.**
Vector component schemas do not set `additionalProperties: false` (serde's `deny_unknown_fields`
is not reflected in the generated JSON Schema). However, the outer wrapper schemas
(`SourceOuter`, `SinkOuter`, etc.) do set `unevaluatedProperties: false`, which has the same
semantic. The coercion pass checks for unknown fields at this level, where it has access to the
complete union of valid properties across the component config and its shared wrapper fields.

**Unknown-field detection is currently non-fatal.** `vector-config` does not yet emit
`#[serde(alias = "...")]` aliases into the generated JSON Schema (tracked TODO in
`vector-config/src/lib.rs`). Until aliases are represented, a key missing from the schema may
still be a legitimate serde alias. The coercion pass logs a warning at the unknown-field path
and defers the authoritative check to serde, which has alias information. When aliases are
emitted in the schema, this can be tightened to a hard error per the original RFC intent.

**Unknown-field checking is skipped for unrecognized component types.** When a component's `type`
value does not match any compiled variant (for example, the component is feature-gated and not
built), the check is suppressed. The component's fields are not in the schema and cannot be
validated. Suppressing avoids false positives and preserves the existing behavior of passing
unrecognized components through to serde, which produces the "unknown variant" error with its
own context.

## Alternatives Considered

**Option A: `CoercingDeserializer` wrapping serde.**
A custom `Deserializer` implementation that intercepts `visit_str` calls and coerces the value to
the type the downstream `Visitor` requests. This keeps all type information inside serde and
avoids a separate schema-walking pass. The approach works for simple structs but breaks for
internally-tagged enums (`#[serde(tag = "type")]`): serde buffers the entire map into an opaque
`Content` value before dispatching to the variant, so the custom deserializer never sees the
nested field types and cannot coerce them. Vector's component configs are pervasively tagged
enums, so this option does not generalize.

**Option B: Run a JSON Schema validator instead of writing a custom pass.**
Run an off-the-shelf JSON Schema validator (for example, the `jsonschema` crate) over the value
tree before deserialization and surface its errors. Validators report structural conformance well
and already produce path-aware errors, but they do not perform coercion: a `"42"` string in an
integer field would hard-fail instead of being converted. Coercing env-var-derived strings to
their declared scalar types is the whole point of the new pass, so a validator alone is not a
substitute. It also cannot account for Vector's custom serde logic (untagged unions, aliases)
that intentionally diverges from the raw JSON Schema.

**Option C: `serde_path_to_error` (or equivalent) on top of serde.**
The `serde_path_to_error` crate annotates serde errors with the field path at which they
occurred, which would solve the "no file/field path" half of the motivation without a
schema-walking pass. It does not solve coercion (string-to-int from env vars still fails at
serde), so it is at best a partial fix. It also inherits the same `Content`-buffering limitation
as Option A on internally-tagged enums: once serde buffers the map for variant dispatch, the path
information is lost for nested fields. Vector's tagged-enum-heavy schema is exactly where
path-aware errors are most needed, so this approach degrades in the cases that matter most.
