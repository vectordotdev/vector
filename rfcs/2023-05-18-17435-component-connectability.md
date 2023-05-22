# RFC 17435 - 2023-05-18 - Component Connectability

Exposing metadata in Vector's configuration schema that allows for more accurately validating that
the components of a Vector configuration are valid and can interoperate with each other.

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- Exposing basic component connectability metadata in the configuration schema: emitted/accept event
  types and named outputs.
- Implementing an API in `vector-config` for parsing the connectability metadata based on a given
  schema and given configuration.
- Implementing an API to define connectability metadata at the per-component level.

### Out of scope

- Building a solution to derive connectability metadata from existing/"normal" Rust code.

## Pain

While consumers of Vector's configuration schema can assert that a configuration is structurally
valid -- it can be deserialized, component names aren't misspelled, non-existent configurations
aren't being set, configuration values are within configured bounds, etc -- it cannot currently
validate that components which are connected together can _be_ connected together.

This means that the configuration schema cannot be used to fully validate a configuration. For
consumers, such as Datadog's Observability Pipelines, it further means that a generated
configuration would need to be run through Vector itself to ensure that it is valid, or even worse,
the logic in Vector would need to be duplicated, leading to potential (and likely) drift in
validation behavior.

## Proposal

### User Experience

In order to encode more of this validation behavior, we propose adding "component connectability"
metadata to the configuration schema. Component connectability specifically refers to constraints
that influence whether or not one component can be connected to another.

For example, sink B may specify source A as an input in the configuration. If the source only emits
logs and the sink can accept logs, this configuration will load successfully and Vector will start.
However, if the source only emits metrics, then Vector would throw an error during startup.

Additionally, many components have constraints that are dependent on the configuration of the
component themselves. Related to the above example, some sinks will potentially accept one or more
event data types depending on the configured encoding, as the ability of the configured encoding to
encode a specific event type is the lowest common denominator for what event types the component can
accept. In other components, their configuration may dictate which outputs they expose, such as the
`route` transform.

Component connectability is the overarching definition of these constraints, modeled as a terse
query syntax, which takes a given configuration, and based on its values, and the query itself,
emits specific computed outputs.

Schema consumers would load these constraint queries, feed the input configuration to them, and
collect the computed outputs. Those computed outputs could then be used to apply further validation
to a configuration, such as ensuring that the set of event data types emitted by source A and the
event data types accepted by sink B overlap completely.

### Implementation

Defining connectability, as described above, revolves around declaring "constraints" on components.
These constraints are a condensed form of the code that we already use in components to logically
define these computed outputs, but reduced to a form that can be serialized and deserialized for
storage within a configuration schema.

In the case of many components, they bear a component type-specific implementation of a
configuration trait, such as all sources implementing the `SourceConfig` trait. These traits are all
primarily designed for actually building the component, but they contain methods for component
type-specific information, such as which outputs the component has (for sources and transforms) or
which event data types are accepted (for transforms and sinks).

For these trait methods, the logic, and the code to encode the logic, is straightforward. Let's look
the `route` transform, specifically its implementation of `TransformConfig::outputs`:

```rust
fn outputs(&self, ...) -> Vec<TransformOutput> {
  let mut outputs = self.route.keys()
    .map(|output_name| TransformOutput::new(DataType::all(), ...).with_port(output_name))
    .collect();
  results.push(TransformOutput::new(DataType::all(), ...).with_port(UNMATCHED_ROUTE));

  results
}
```

We've condensed the original code somewhat, but the essence is still intact: for all configured
routes (which come from `self.route`), take the key (which is the name of the route) and configure
an output that can emit all possible event data types. Additionally, add an "unmatched" output where
all events that failed to match a configured route will be sent to.

This logic is straightforward, and when explained as it is above, one could read the configuration
of a `route` transform and manually calculate the named outputs that the transform would expose.
Encoding that operation, the process that we might mentally perform to calculate the named outputs,
is what the constraint language is designed around.

#### Defining constraints through composition of high-level operations

The proposed constraint language represents an amalgam of boolean operators and full-blown
programming language opcodes. As we need to represent many operations which could inherently be
represented when using the full power of Rust, we take an approach of defining operators that
capture the higher-level operation being performed.

Here is a slimmed down version of `Computed`, the core primitive for defining a constraint:

```rust
/// A computed constraint.
pub enum Computed {
    /// A fixed value.
    Fixed(Value),

    /// An array of computed values.
    Array(Vec<Computed>>),

    /// Flattens array-of-arrays into a single array.
    Flatten(Box<Computed>),

    /// A value that depends on the result of a conditional.
    Optional {
        condition: Condition,
        value: Box<Computed>,
    },

    /// A value that is derived from the input itself.
    Derived {
        target: InstancePath,
        operation: DeriveOp,
    },

    /// A value that is retrieved from a lookup table.
    Lookup {
        key: Box<Computed>,
        table: HashMap<String, Computed>,
    }
}

impl Computed {
    /// Computes the output of this value.
    pub fn compute(&self, input: &Value) -> Option<Value> {
        match self {
            Computed::Fixed(v) => Some(v.clone()).and_then(nonnull_or_none),
            Computed::Flatten(value) => flatten_value(value),
            Computed::Optional { condition, value } => condition
                .check(input)
                .then_some(value.compute(input))
                .flatten(),
            Computed::Array(items) => {
                Some(items.into_iter().filter_map(|c| c.compute(input)).collect())
            }
            Computed::Derived { target, operation } => target.lookup(input)
                .and_then(|value| operation.derive(value)),
            Computed::Lookup { key, table } => key.compute(input)
                .and_then(|v| v.as_str().and_then(|key| table.get(key)))
                .and_then(|v| v.compute(input)),
        }
    }
}
```

Constraints are modeled in a nested fashion, and are defined as operations: return a fixed value,
derive the value from the input, look up the value from a lookup table, and so on. These operations,
as defined above, are already flexible enough to encode nearly all use cases for defining component
connectability metadata, as the flexibility necessary for defining these constraints in Rust is less
about having full access to all of Rust's syntax and more about having dynamic access to the input
data while also specifying some fixed data (such as lookup tables).

Additionally, constraints have an implicit notation of being "computed" from the actual input
configuration data. This models the logic of the definition of these constrains -- such as defined
by implementations of `TransformConfig::outputs`, etc -- running against `&self`, where `&self` is
the deserialized equivalent of the raw configuration data for a given component.

Let's briefly look at an example of modeling the above `route` example using `Computed`:

```rust
impl RouteConfig {
  fn outputs_constraint(&self, path: InstancePath) -> Computed {
    // Return a flattened array, since otherwise we'd end up with [[derived keys], "_unmatched"].
    Computed::flatten(Computed::array([
        // Extract the property names (keys) of all properties within the `route` property of the route configuration.
        Computed::derived(path.push("route"), DeriveOp::Keys),

        // Add a fixed route of "_unmatched".
        Computed::fixed(UNMATCHED_ROUTE),
    ]))
  }
}
```

We've contorted the code a little here for the sake of example (how the code is actually composed
together and proposed to be used will be shown later), but again, the essence is still intact. We've
defined a method that builds a computed constraint which, given the route configuration, will look
up the `route` property and extract any property names if `route` is an object, and then add a fixed
item of "_unmatched" to the list, and then finally it flattens those two things together so that the
return value looks like `["string 1", "string 2", ... "string N", "_unmatched"]`.

Since we mentioned the route configuration itself, let's define an example route configuration that
we'll run this computed constraint against:

```yaml
transforms:
  router:
    type: route
    inputs: ["datadog_agent"]
    route:
      logs_only:
        type: is_log
      metrics_only:
        type: is_metric
```

As mentioned above, the constraint is run with the route configuration as its input, because
constraints are tied specifically to components. When `Computed::compute` is called, it is given an
input that contains the value defined by `transforms.router`, as in, the value is _rooted_ at
`transforms.router`. When the constraint thus tries to find a target path of `route`, it is getting
back an object with the properties `logs_only` and `metrics_only`.

Computing this constraint would give us an output of `["logs_only", "metrics_only", "_unmatched"]`.

The overarching premise here is that the enum variants of `Computed` effectively map to a
higher-level operation -- grab a value from the input, include a value conditionally if another
field is set to a specific value, etc -- that we would otherwise specify in Rust, and `Computed`
is evaluated against the input configuration data _for the component_ in the same way that,
following from the `route` transform example, `TransformConfig::outputs` would be called on the
deserialized `RouteConfig` object.

#### Serializing constraints for inclusion in the configuration schema

The demonstration of `Computed` above elides one major thing: `Computed` is serializable.

In conjunction with a simple helper type, `Constraints`, which represents a map of computed
constraints, constraints are meant to be serialized and deserialized to and from a configuration
schema. Here's a simple example of the constraints for the `route` transform when serialized:


```json
```

#### Coupling constraints to components in source

We've laid out a rough sketch of how we propose to

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
