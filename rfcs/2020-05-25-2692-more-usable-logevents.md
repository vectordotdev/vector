# More Usable LogEvents

Improve Vector's `LogEvent` type with some small changes based on lessons from our current work and production experience.

## Motivation

The `LogEvent` API has a long history in Vector. Indeed, it is one of our most fundamental types. In the average Vector run thousands of `LogEvent` lifetimes occur.

During recent benchmarking and performance investigations, we started to dig into the performance characteristics of this structure, and think we can make some performance and ergonomics improvements.

Vector defines success by the highest number of `Event` lifetimes to have been sourced, processed, and then sunk as possible, while maintaining practical levels of reliability and safety.

**If necessary:** Providing a simple, ergonomic API into an internally complex and performant structure is a **worthy trade-off** given the nature of this structure.

For the most part, `LogEvent`s are fairly small, uncomplicated structures, containing a few keys with simple values. Here is a simple example:

```json
{
    "message": "I found a potato!",
    "potato_harvester_id": 123,
    "potato_location": { "x": 123, "y": 456, }
}

```

Not so bad, right?

### A Motivatingly Complex Example

Before we can fully grip at our internal `LogEvent` type, let's review a particularly complex example JSON to help us understand where our limits might be:

```json
// Note, this does not cover all possibilities!
{
    "nulled": null, // Ensure a nulled field is not lost.
    "basic": true,  // Maps can contain multiple types.
    "list": [
        true, // Lists can contain multiple types.
        null, // A null in a list is totally valid.
        [true, null, true],
        {
            "basic": true,
            "buddy": 1.0, // Maps with multiple values are a bit more complex.
        },
    ],
    "map": {
        "basic": true,
        "list": [true, null, true],
        "map": {
            "basic": true,
            "buddy": -1,
        }
  },
}

```

### Dogma and Intent

Vector has embraced the idea of **Lookups**, which are analogous to CSS Selectors or `jq` selectors, that can represent an arbitrary selection of value(s). It is irrelevant to the user how the `LogEvent` is internally structured, so long as they can utilize these Lookups to work with a `LogEvent`.

The `LogEvent` variant used under a two primary intents:

* **Programmatically, through our internal code.** When used in this way, the `LogEvent` is being interacted with through static, safe interfaces and we can utilize the full type system available to us.
* **Through configuration or FFI via knobs or scripts.** When used this way, the configuration format (at this time, TOML) may impact how aesthetic or expressive our configuration is. For example, TOML values like `a.b.c` as defined in a configuration. This is TOML table syntax, and we can't just magically treat it like a `Lookup`, a conversion step is necessary.

### Understanding `LogEvent` in Vector 0.9.0

Before we explore how to improve `LogEvent`, we must make sure we're familiar with the current one! Let's review the structure and some examples of lifetimes of this structure.

#### `LogEvent` Internal Structure

The `LogEvent` currently meets our needs from several ergonomics angles (notably the flattening code), but we often must twist it in painful ways to work with it (notably buffers to flatmaps to `BTree` style APIs).

```rust
//! src/event/mod.rs
#[derive(PartialEq, Debug, Clone)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Timestamp(DateTime<Utc>),
    Bytes(Bytes),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>)
}
```

The current `LogEvent` type's internal data model bears semblance to the [`serde_json::Value::Value::Object(Map<String, Value>)`](https://docs.serde.rs/serde_json/value/enum.Value.html) variant.

```rust
//! serde_json::value::Value
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}
```

Differences are:

* A `Bytes` type in our `Value` type exists because `serde_json::value::Value` and most JSON implementations support
  this type through the `String` type, which isn't ideal for applying `Bytes` related functions to. We hold this special
  `Bytes` variant to support better in-pipeline processing for Vector.
* Our `Value` type lacks a `String` variant, using the `Bytes` variant instead.
* A `Timestamp` type in our `Value` type exists for largely the same reason the `Bytes` variant exists: Optimize in-pipeline processing.
* Separate `Float` and `Integer` type exist whereas `serde_json::value::Value::Number`, also for optimizing in-pipeline processing.

#### `LogEvent` API Functionality of Note

The `LogEvent` API implements a pseudo- `Flatmap` API. It uses special `PathIter` types to parse `String` or `Atom` keys provided to the APIs (`get`, `insert`, etc).

The `Atom` type is deprecated, please use `String` and adapt any `Atom` usages to `String` where possible. See [#1891](https://github.com/vectordotdev/vector/pull/1891).

The current `LogEvent` API is as follows *(documentation, if it exists, is included)*:

```rust
//! Summarized from src/event/mod.rs
impl LogEvent {
    pub fn new() -> Self;
    pub fn get(&self, key: &Atom) -> Option<&Value>;
    pub fn get_mut(&mut self, key: &Atom) -> Option<&mut Value>;
    pub fn contains(&self, key: &Atom) -> bool;
    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<Value>
        where K: AsRef<str>, V: Into<Value>;
    pub fn insert_path<V>(&mut self, key: Vec<PathComponent>, value: V) -> Option<Value>
        where V: Into<Value>;
    pub fn insert_flat<K, V>(&mut self, key: K, value: V)
        where K: Into<String>, V: Into<Value>;
    pub fn try_insert<V>(&mut self, key: &Atom, value: V)
        where  V: Into<Value>;
    pub fn remove(&mut self, key: &Atom) -> Option<Value>;
    pub fn remove_prune(&mut self, key: &Atom, prune: bool) -> Option<Value>;
    pub fn keys<'a>(&'a self) -> impl Iterator<Item = String> + 'a;
    pub fn all_fields<'a>(&'a self) -> impl Iterator<
                Item = (String, &'a Value)
        > + Serialize;
    pub fn is_empty(&self) -> bool;
}
```

Many of these APIs offer functionality ***coarsely similar*** to a `FlatMap`. These APIs were added ad-hoc, as needed over time, and were not designed intentionally to all fit together. In addition, some changes were added unobtrusively to aid in the migration path, such as `insert_path` and `insert_flat`, or the differing outputs of `keys` and `all_fields`.

#### Example `LogEvent` Lifetimes

* **Simple Connector:** A `LogEvent` entering Vector through a `http(encoding=json)//console(encoding=text)` pipeline.
* **Runtime Pipeline:** A `LogEvent` entering Vector through a `http(encoding=json)/lua/http(encoding=json,buffer=disk)` pipeline.
* **N:M:O sources, transforms, and sinks:** A `LogEvent` entering Vector through a `???/????/???` pipeline.

## Guide Level Proposal

There is no guide accompanying this RFC, it only minimally touches user facing surface.

## Doc Level Proposal

> **Placement:** Insert into [Log Event](https://vector.dev/docs/about/data-model/log/#types)'s [Types](https://vector.dev/docs/about/data-model/log/#types) section

### Bytes

An arbitrary sequence of bytes (not necessarily UTF-8), bounded by system memory.

> **Placement:** Modify the [Coercer transform](https://vector.dev/docs/reference/transforms/coercer/#field-name) Types section.

```bash
Enum, must be one of: "bool" "float" "int" "string" "timestamp", "bytes"
```

## Prior Art

In the [WASM transform](https://github.com/vectordotdev/vector/pull/2006/files) our demo POC was using protobufs, which by their nature are not always UTF-8. We had to patch Vector to support Bytes during this time. It currently does so via a fallback when non-UTF-8 bytes are found.

## Sales Pitch

This RFC ultimately proposes the following steps:

1. Add UX improvements on `LogEvent`, particularly turning JSON into or from `LogEvent`.
1. Refactor the `PathIter` to make `vector::event::Lookup` type.
1. Add UX improvements on `Lookup` , particularly an internal `String` ↔ `Lookup` with an `Into`/`From` that does not do path parsing, as well as a `<Lookup as std::str::FromStr>::from_str(s: String)` that does. (This also enables `"foo.bar".parse::<Lookup>()?`)
1. Refactor all `LogEvent` to accept `Into<Lookup>` values.
    1. Remove obsolete functionality like `insert_path` since the new `Lookup` type covers this.
    2. Refactor the `keys` function to return an `Iterator<Lookup>`
1. Add an `Entry` style API to `LogEvent`.
    1. Remove functionality rendered obsolete by the Entry API like `try_insert`, moving them to use the new Entry API
1. Provide `iter` and `iter_mut` functions that yield `(Lookup, Value)`.
    1. Remove the `all_fields` function, moving them to the new iterator.

We believe these steps will provide a more ergonomic and consistent API.

## Drawbacks

The changes to `Path` and `PathIter` may lead to confusion or footgunning. We **must** focus on ensuring safety from ourselves when designing this API.

Supporting `Value::Bytes` means we may take on additional maintenance burden and have to make new transforms that deal with bytes.

Some naming/API changes, may produce a small mental burden on our developers, but our team can communicate and overcome this.

## Rationale & Alternatives

While we have bigger plans for the event type, such as specialized metadata, maintaining raw representations, etc, those all depend on having a consistent, easy to reason about `LogEvent` types.

This RFC is conservative in that it does not actually suggest any of those steps, and simply focuses on setting a stage for those to more easily happen.

We may, **alternatively**, decide to scope out more work for this RFC, including exploring CoW based fanout, simd-json, etc.

## Outstanding Questions

* Should any of these steps *not* be taken?
* Should any of these steps be taken differently?
* Should we take these steps in a different order?

## Plan Of Attack

The steps taken, in the order to be taken, are listed in "Sales Pitch".
