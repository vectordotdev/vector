# RFC <issue#> - <YYYY-MM-DD> - The Vector Engine

Introduce the Vector Engine, an ergonomic runtime supporting the WASM (and WASI) compile targets.


## Motivation

There is a plurality of reasons for us to wish for flexible, practical WASM support in Vector.

Since the concept of a WASM engine is very abstract, this section will instead discus a number of seemingly unrelated
topics, all asking for different features. Later, we'll see how wasm helps us.


### Protobufs & other custom codecs

We noted in discussions with several current and prospective users that custom protobuf support in the form of an
`encoding` option or a transform.

Protobufs in their `proto` form aren't usable usable by the prevailing Rust Protobuf libraries in a dynamic way. Both
`serde` and `rust-protobuf` focus almost solely on compiling support for specific Protobufs **into** a Rust program.
While a `serde-protobuf` crate exists, we found it to be an order of magnitude slower than the other two options, we
also noted it lacked serialization support.

While evaluating the possibility of adding serialization we noted that any implementation that operated dynamically
would be quite slow. Notably, it would **it would seriously impact overall event processing infrastructure** if Vector
was responsible for a slowdown. These organizations are choosing to use protobufs usually for speed and/or compatibility
guarantees.

We should enable and encourage these goals, our users should be empowered to capture their events, and not worried about
processing load. So the question became: *How do we let users opt-in to custom protobuf support efficiently and without
great pain?*


### Increasing build complexity and our slow build times

Recently, we've been discussing the trade offs associated with different code modularization techniques. We currently
make fairly heavy use of the Rust feature flag system.

This is great because it enables us to build in multi-platform support as well as specialized (eg only 1 source) builds.
We also noted builds with only 1 or two feature flags are **significantly** faster than builds with all features
supported.

One downside is our current strategy can require tests (See the `make check-features` task) to ensure their correctness.
The other is that we sometimes need to spend a lot of time weaving through `cfg(feature = "..")` flags.

Having some solution to cleanly package independent modules of functionality would allow us to have a core Vector which
builds fast while keeping modularity features.


### Language runtime support

Vector currently bundles a [`lua` transform](https://vector.dev/docs/reference/transforms/lua/), and there is ongoing
work on a [`javascript` transform](https://github.com/timberio/vector/issues/667).

Currently, each language requires as separate runtime, and possibly separate integration subtleties like needing to call
a [GC](https://github.com/timberio/vector/pull/1990). It would be highly convenient if we could focus our efforts on one
consistent API for all languages, and spend this time saved providing a better, faster, more safe experience.


### A Plugin System

As Vector grows and supports more integrations, our binary size and build complexity will inevitable grow. In informal
discussions we've previously noted the [module system of Terraform](https://www.terraform.io/docs/commands/init.html).
This kind of architecture can help us simplify the codebase, reduce binary sizes, formalize our APIs, and open Vector up
to a third-party module ecosystem.

Terraform typically runs on CIs and developer machines, making portability valuable. For Terraform, users can follow the
[guide](https://www.terraform.io/docs/extend/writing-custom-providers.html) to write a Go-based provider, which they can
then distribute as a portable binary. Notably: This means either distributing an unoptimized binary, distributing a lot
of optimized binaries, or requiring folks build their own optimized binaries. Terraform doesn't care much about speed,
so unoptimized binaries are acceptable.

Vector is in a slightly different position than Terraform though! Vector runs primarily in servers and even end user
machines. We can't expect users to have build tooling installed to their servers (it's a security risk!) and we
definitely can't expect it on an end-user machines. Most people just aren't that interested in computers. Vector has
different performance needs, too. While folk's aren't generally wanting to execute terraform providers hundreds of
thousands (or millions) of times per second, they are absolutely doing that with Vector.

When we're processing the firehose of events originating from a modern infrastructure every millisecond counts. Vector
needs a way to ship portable, *optimizable* modules if we ever hope of making this a reality.


### Simplifying common basic transforms

We would like to avoid asking users to chain together a number of transforms to accomplish simple tasks. This is
evidenced by issues like [#750](https://github.com/timberio/vector/issues/750),
[#1653](https://github.com/timberio/vector/issues/1653), and [#1926](https://github.com/timberio/vector/issues/1926).
Having to, for example, use two transforms just to add 1 field and remove another is very verbose.

We noted that the existing lua runtime was able to accomplish these tasks quite elegantly, however it was an order of
magnitude slower than a native transform.

> TODO: Proof

Users shouldn't pay a high price just for a few lines saved in a configuration file. They shouldn't feel frustration
when building these kinds of pipelines either.


### Exploring DSL driven pipelines

We've discussed various refinements to our pipelines such as
[Pipelines Proposal V2](https://github.com/timberio/vector/issues/1679) and
[Proposal: Compose Transform](https://github.com/timberio/vector/issues/1653). Some of these discussions have been about
improving our TOML syntax, others about new syntax entirely, and others about how we could let users write in the
language of their choice.

The path forward is unclear, but if we choose to adopt something new we must focus on providing a good user experience
as well as performance. Having a fast, simple, portable compile target could make this an easier effort.


### Ecosystem Harmony with Tremor

We exist in an ecosystem, [Tremor](https://www.tremor.rs/) exists and we'd really love to be able to work in harmony
somehow. While both tools process events, Vector focuses on acting as a host-level or container-level last/first mile
router, Tremor focuses on servicing demanding workloads from it's position as an infrastructure-wide service.

What if we could provide users of Tremor and Vector with some sort of familiar shared experience? What if we could share
functionality? What kind of framework could satisfy both our needs? There are so many questions!

> TODO: We need to talk to them.


## Prior Art

We have an existing `lua` runtime existing as transform, and there is currently an
[issue](https://github.com/timberio/vector/issues/667) regarding an eventual `javascript` transform. Neither of these
features reach the scope of this proposal, still, it is valuable to learn.

While evaluating the `lua` transform to solve the Protobuf problem described in Motivations, we benchmarked an
implementation of `add_fields` that was already performing at approximately the same performance as the `serde-protobuf`
solution.

We also noted that the existing `lua` transform is quite simple. While this makes for nice usability, it means things
like on-start initializations aren't possible. While these kinds of features may be added, this shouldn't stop us from
investigating WASM.

We also noted the capabilities and speed of WASM (particularly WASI) mean we could eventually support things like
sockets and files.

Indeed, it is possible we will be able to let Vector build and run Lua code as a wasm module in the future.


## Guide-level Proposal

Vector contains a WebAssembly (WASM) engine which allows you to write custom functionality for Vector. You might already
be familiar with the [Javascript](TODO) or [Lua]() transforms, WebAssembly is a bit different than that.


### When to build your own module

Using the WASM engine you can write your own sources, transforms, sinks, codecs in any language that compiles down to a
`.wasm` file. Vector can then compile this file down to an optimized, platform specific module which it can use. This
allows Vector to work with internal or uncommon protocols or services, support new language transforms, or just write a
special transform to do *exactly* what you want. With the WASM engine, Vector puts you in command.

There is a trade-off though! You'll need to embolden yourself, as WASM is a fledgling technology. When it first loads a
WASM file, Vector needs to spend some time building an optimized module before it can load it. You'll also be
responsible for the safety and correctness of your code, if your module crashes on an event, Vector will re-initialize
the module and try again on the next event.

Here's some examples of when a WASM module is a good fit:

* You need to support a specific protobuf type.
* You have a source or sink which is not currently supported by Vector.
* You want to write a complex transform in your favorite language.

Here's some examples of where WASM probably isn't right for you:

* You need functionality already offered by Vector.
* Your desired language doesn't support WASM as a compile target.
* You are using Vector on a non-Linux X86-64 bit compatible target. (Lucet, our Engine, only supports Linux at this
current time. Work on other operating systems is ongoing!)
* You need a battle-proven system, WASM support in Vector is young!

### How Vector modules work

Vector modules are `.wasm` files (WASI or not!) exposing a predefined interface. We provide a `vector::api::guest`
module for Rust projects, and we will be providing bindings for other languages as we see more user interest.

Modules are sandboxed and can only interact with Vector over the Foreign Function Interface (FFI) which Vector exposes.
For each instance of a module, Vector holds some context. For example, when a transform module processes an event, the
context of the module contains an `Event` type. The guest can use calls like `get("foo")` and Vector perform the action
and return the result to the guest.

A guest module is not able to access the memory of the Vector instance, but Vector is free to modify the guest memory.
For example, behind the scenes of a `get` call, the Guest must allocate some memory for Vector to write the result into.

Due to the current semantics of the `vector::Event` type, passing data between WASM modules and the Vector host requires
serialization. We're investigating ways to speed this up and this API is designed to try to keep trade-offs minimal.
This is why `process()` is not `process(e: Event)`, it would require serializing and deserializing every Event.


### Setup


Regardless of whether of not the module uses WASI, or has access to a language specific binding, it can interact with
Vector. Vector works by invoking the modules public interface. First, as soon as the module is loaded, `register` is
called once, globally, per module. In this call, the module can configure how Vector sees it and talks to it.

If using the Vector guest API this can be done via:

```rust
use vector::api::guest::{Registration, roles::Sink};
#[no_mangle]
pub extern "C" fn register() {
    let stub = Default::default();
    Registration::<Sink>::default()
        .blanket_option(stub)
        .sink_only_option(stub)
        .register();
}
```

Then, when each module is instantiated for the first time `initialize` is called. During this time, the module can do
any one time setup it needs. (Eg. Opening a socket, a file).

```rust
const ONE_TIME_INIT: Once = Once::new();
#[no_mangle]
pub extern "C" fn initialize() {
    ONE_TIME_INIT.call_once(|| {
        println!("One time init");
    })
}
```

What happens next depends on which type of module you register.

<details>
  <summary>Expand for API Def</summary>

  ```rust
  pub mod vector {
      pub mod api {
          pub mod guest {
              use roles::*;
              pub mod roles {
                  pub trait Role: Default { /* ... */ }

                  #[derive(Default)]
                  pub struct Transform;
                  impl Role for Transform { /* ... */ }

                  #[derive(Default)]
                  pub struct Sink;
                  impl Role for Sink { /* ... */ }
                  /* ... */
              }

              #[derive(Default)]
              #[must_use]
              pub struct Registration<R> where R: Role { role: R, }

              impl<R> Registration<R> where R: Role {
                  pub fn blanket_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
                  /* ... */

                  pub fn register(&mut self) -> Result<(), usize> { unimplemented!() }
              }

              impl Registration<Transform> {
                  pub fn transform_only_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
                  /* ... */
              }

              impl Registration<Sink> {
                  pub fn sink_only_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
                  /* ... */
              }

              pub mod hostcall {
                  use serde_json::Value;
                  pub(crate) fn get(field: impl AsRef<str>) -> Result<Option<Value>, usize> { unimplemented!() }
                  pub(crate) fn insert(field: impl AsRef<str>, value: impl Into<Value>) -> Result<(), usize> { unimplemented!() }
                  /* ... */
              }

              /* ... */
          }
      }
  }
  ```

</details>

### Module types

An avid Rust hacker will note the below APIs are not very idiomatic!

> TODO: Reflect some of the practice from
https://michael-f-bryan.github.io/rust-ffi-guide/errors/return_types.html

* `Transform`: Modules of this time can be used as a transform `type`.

  ```rust
  #[no_mangle]
  pub extern "C" fn process() -> usize {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
      0
  }
  ```

* `Source`: Modules of this time can be used as a source `type`.
> The rest are still a big TODO. How can we handle batching and partitioning etc? How do we handle all the knobs?

  ```rust
  #[no_mangle]
  pub extern "C" fn start() {
      // TODO
  }
  ```

* `Sink`: Modules of this time can be used as a sink `type`.
> The rest are still a big TODO. How can we handle batching and partitioning etc? How do we handle all the knobs?

  ```rust
  #[no_mangle]
  pub extern "C" fn process() {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
  }
  ```

* `Codec`:
> The rest are still a big TODO. How can we handle batching and partitioning etc? How do we handle all the knobs?

  ```rust
  #[no_mangle]
  pub extern "C" fn process() {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
  }
  ```


### Your first module (Rust)

To create your first module, start with a working Rust toolchain (see https://rustup.rs/) add the `wasm32-wasi`
toolchain's `nightly` version:

```bash
rustup target add wasm32-wasi --toolchain nightly
```

Then, create a module:

```bash
cargo +nightly init --lib echo
```

In your `Cargo.toml` fill in:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
vector = "0.8.2"
```

Next, in your `lib.rs` file:

```rust
use vector::api::guest::{Registration, roles::Transform, get, insert};

#[no_mangle]
pub extern "C" fn register() {
    Registration::<Transform>::default()
        // TODO: Options here
        .register();
}

#[no_mangle]
pub extern "C" fn initialize() {
    // Relax! Nothing needed.
}


#[no_mangle]
pub extern "C" fn process() -> usize {
    let result = vector::get("message");

    println!("From inside the wasm machine: {:?}", result);
    // TODO: Do your stuff

    match result.unwrap() {
        Some(value) => {
            vector::insert("echo", value);
            0
        }
        None => {
            0
        }
    }
}
```

Now to build it:

```bash
cargo +nightly build --target wasm32-wasi --release
```

In your `target/wasm-wasi/release/` verify that the `echo.wasm` file exists. Next edit your Vector `config.toml`:

> TODO: Talk more about this!

```toml
data_dir = "/var/lib/vector/"
dns_servers = []

[sources.source0]
max_length = 102400
type = "stdin"

[transforms.demo]
inputs = ["source0"]
type = "wasm"
source = "$TARGET_DIRECTORY/echo.wasm"

[sinks.sink0]
healthcheck = true
inputs = ["demo"]
type = "console"
encoding = "json"
buffer.type = "memory"
buffer.max_events = 500
buffer.when_full = "block"

[[tests]]
  name = "demo-tester"
  [tests.input]
    insert_at = "demo"
    type = "log"
    [tests.input.log_fields]
      "message" = "foo"
  [[tests.outputs]]
    extract_from = "demo"
    [[tests.outputs.conditions]]
      "echo.equals" = "foo"
```

Now try `vector test config.toml` and Vector will go ahead and build an optimized `cache/echo.so` artifact then run it
for a unit test.

```bash
Running test.toml tests
test test.toml: demo-tester ... passed
```


### Resources & templates

* [Rustinomicon](https://doc.rust-lang.org/nomicon/)
* [Lucet Docs](https://bytecodealliance.github.io/lucet/)
* [WASI](https://wasi.dev/)


## Sales Pitch

Integrating a flexible, practical, simple WASM engine means we can:

* Expose one common interface to any existing and future languages.
* Allow large-scale users to use custom protobufs.
* Support and advertise a plugin system.
* Empower us to modularize Vector's components and increase maintainability.
* Share a common plugin format with Tremor.
* In the future, support something like AssemblyScript as a first class language.


## Drawbacks

WASM is still **very young**. We will be early adopters and will pay a high adoption price. Additionally, we may find
ourselves using unstable functionality of some crates, including a non-trivial amount of `unsafe`, which may lead to
unforseen consequences.

Our **binary size will bloom**. While only proofs of concept are complete, we could see our binary size grow by over 30
MB (TODO).

Our team will need to diagnose, support, and evolve an arguably rather **complex API** for WASM modules to work
correctly. This will mean we'll have to think of our system very abstractly, and has many questions around API stability
and compatibility.

This feature is **advanced and may invoke user confusion**. Novice or casual users may not fully grasp the subtleties
and limitations of WASM. We must practice caution around the user experience of this feature to ensure novices and
advanced users can understand what is happening in a Vector pipeline utilizing WASM modules.


## Outstanding Questions

Since this is a broad reaching feature with a number of green sky ideas, we should discuss these questions:


### Agree on breadth and depth of implementation

We should support modules so broadly?

* As sinks? For this we should consider how we handle batching, partitioning, etc, more.
* As sources? This requires thinking about how we can do event sourcing.
* What about the idea of codecs being separate?
* This RFC makes some provisions for a future Event refactor, is it possible that this might happen?


### Consider supporting a blessed compiler

Our original discussions included the idea of a blessed compiler and a UX similar to our existing `lua` or `javascript`
transform. Indeed, our initial implementation should include just a transform, as it's by far the easiest place to
introduce this feature.

During investigation of the initial POC we determined that our most desirable "blessed" language would be
AssemblyScript, unfortunately, it's currently not able to run inside Lucet. Work is ongoing and we expect it to be
possible soon.


### Consider API tradeoffs

We should consider if we are satisfied with the idea of hostcalls being used for `get` and other API. We could also let
the host call the Guest allocate function and then pass it a C String pointer to let it work on. This, however, requires
serializing and deserializing the entire Event each time, which is a huge performance bottleneck.

We could also consider adopting a new strategy for our `event` type. However, that work is outside the scope of this
RFC. The current structure of the `Event` type is already discussed in
[#1891](https://github.com/timberio/vector/issues/1891).

We should also consider if we want to change how we handle codecs, since it is likely that WASM module use cases will
include wanting to add codec support to already existing sources.


### Consider Packaging Approach

We will need to expose some Vector guest API to users. This will require ABI compatibility with Vector, so it makes
sense to keep it in the same repository (and maybe the same crate?) as Vector. We can consider either making Vector able
to be built by WASI targets (more feature flags) or using a workspace and having the guest API as a workspace member we
publish.

A couple good questions to consider:

* Do we one day want to support a cross platform `cargo install vector` option for installing a Vector binary?
* Should a user import `vector::...` to use the Vector guest API in their Rust module?
* Vector's internal API is largely undocumented and kind of a mess. Should we hide/clean the irrelevant stuff?

### Consider observability

How can we let users see what's happening in WASM modules? Can we use tracing somehow? Lucet supports tracing, perhaps
we could hook in somehow?

## Plan of attack

Incremental steps that execute this change.

- v0.1: This RFC is accepted forming the basis for the RFC, and demoing a POC of how a theoretical user could use this
  for a protobuf decoding transform, and permitting the first `engine` unit test to pass.
- This RFC is amended to include Sinks and Sources information.
- This RFC is amended to include information about possible codec changes.

Note: This can be filled out during the review process.
