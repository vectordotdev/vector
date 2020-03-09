# RFC <issue#> - <YYYY-MM-DD> - The Vector Engine

Introduce the Vector Engine, an ergonomic runtime supporting the WASM (and WASI) compile targets.

## Motivation

There is a plurality of reasons for us to wish for flexible, practical WASM support in Vector.

Since the concept of a WASM engine is very abstract, this section will instead discus a number of seemingly unrelated topics.

### Protobufs & other custom codecs

We noted in discussions with several current and prospective users that custom protobuf support in the form of an `encoding` option or a transform.

Protobufs in their `proto` form aren't usable usable by the prevailing Rust Protobuf libraries in a dynamic way. Both `serde` and `rust-protobuf` focus almost solely on compiling support for specific Protobufs **into** a Rust program. While a `serde-protobuf` crate exists, we found it to be an order of magnitude slower than the other two options, we also noted it lacked serialization support.

While evaluating the possibility of adding serialization we noted that any implementation that operated dynamically would be quite slow. Notably, it would **it would seriously impact overall event processing infrastructure** if Vector was responsible for a slowdown. These organizations are vhoosing to use protobufs usually for speed and/or compatibility guarantees.

We should enable and encourage these goals, our users should be empowered to capture their events, and not worried about processing load. So the question became: *How do we let users opt-in to custom protobuf support efficiently and without great pain?*

### Increasing build complexity and our slow build times

Recently, we've been discussing the tradeoffs associated with different code modularization techniques. We currently make fairly heavy use of the Rust Feature Flag system.

This is great because it enables us to build in multi-platform support as well as specialized (eg only 1 source) builds. We also noted builds with only 1 or two feature flags are **significantly** faster than builds with all features supported.

One downside is our current strategy can require tests (See the `make check-features` task) to ensure their correctness. The other is that we sometimes need to spend a lot of time weaving through `cfg(feature = "..")` flags.

### Language runtime support

### A Plugin System

### Exploring DSL driven pipelines

### Ecosystem Harmony with Tremor

### Simplifying common basic transforms

## Prior Art

We have an existing `lua` runtime existing as transform, and there is currently an issue (TODO: Link) regarding an eventual `javascript` transform. Neither of these features reach the scope of this proposal, still, it is valuable to learn.

While evaluating the `lua` transform to solve the Protobuf problem described in Motivations, we benchmarked an implementation of `add_fields` that was already performing at approximately the same performance as the `serde-protobuf` solution.

We also noted that the existing `lua` transform is quite simple. While this makes for nice usability, it means things like on-start initializations aren't possible. While these kinds of features may be added, this shouldn't stop us from investigating WASM.

Indeed, it is possible we will be able to let Vector build and run Lua code as a wasm module in the future.

## Guide-level Proposal

### When to build your own module

### How Vector modules work

### Your first module (Rust)

### Module types

* `Transform`:
* `Source`:
* `Sink`:
* `Codec`:
* `Language`:

### Resources & templates


## Sales Pitch

Integrating a flexible, practical, simple WASM engine means we can:

* Expose one common interface to any existing and future languages.
* Allow large-scale users to use custom protobufs.
* Support and advertise a plugin system.
* Empower us to modularize Vector's components and increase maintainability.
* Share a common plugin format with Tremor.
* In the future, support something like Assemblyscript as a first class language.


## Drawbacks

WASM is still **very young**. We will be early adopters and will pay a high adoption price. Additionally, we may find ourselves using unstable functionality of some crates, including a non-trivial amount of `unsafe`, which may lead to unforseen consequences.

Our **binary size will bloom**. While only proofs of concept are complete, we could see our binary size grow by over 30 MB (TODO).

Our team will need to diagnose, support, and evolve an arguably rather **complex API** for WASM modules to work correctly. This will mean we'll have to think of our system very abstractly, and has many questions around API stability and compatibility.

This feature is **advanced and may invoke user confusion**. Novice or casual users may not fully grasp the subtleties and limitations of WASM. We must practice caution around the user experience of this feature to ensure novices and advanced users can both understand what is happening in a Vector pipeline utilizing WASM modules.

## Outstanding Questions

### Agree on breadth and depth of implementation

### Consider supporting a blessed compiler

###

## Plan of attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.
