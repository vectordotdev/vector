# RFC <issue#> - <YYYY-MM-DD> - WASM Foreign Module Support

Introduce Vector's foreign module support, a way for users to add custom functionality to Vector. This RFC introduces
WASM modules specifically, but we envision the possibility of other implementations (these will be a different RFC).
This RFC introduces Transforms and will be later amended with Source and Sink functionality.


## Motivation

There is a plurality of reasons for us to wish for flexible, practical foreign module support to Vector. These may also
 be regarded as 'Plugins' or 'Extensions.' WASM is the most versatile and obvious choice for a first implementation as
 it can be targetted by a number of languages, and is intended to be OS, arch, and platform independent.

Since the concept of a foreign module is very abstract, this section will instead discuss a number of seemingly
unrelated topics, all asking for different features. While foreign module support does not resolve all these problems,
it gives us or our users a path to a solution.


### Protobufs & other custom codecs

We noted in discussions with several current and prospective users that custom protobuf support in the form of an
`encoding` option or a transform.

Protobufs in their `proto` form aren't usable by the prevailing Rust Protobuf libraries in a dynamic way. Both
`prost` and `rust-protobuf` focus almost solely on compiling support for specific Protobufs **into** a Rust program.
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
machines. We can't expect users to have build tooling installed on their servers (it's a security risk!) and we
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

We also noted that the existing `lua` v1 transform was quite simple. While this makes for nice usability, it means
things like on-start initializations aren't possible. In [v2](https://github.com/timberio/vector/pull/2000) `lua` has
learnt many new tricks, and this WASM support aims to match them.

We also noted the capabilities and speed of WASM (particularly WASI) mean we could eventually support things like
sockets and files.

Indeed, it is possible we will be able to let Vector build and run Lua code as a wasm module in the future.


## Guide-level Proposal

Vector contains a WebAssembly (WASM) engine which allows you to write custom functionality for Vector. You might already
be familiar with the [Javascript](https://github.com/timberio/vector/pull/721) or
[Lua](https://vector.dev/docs/reference/transforms/lua/) transforms, WebAssembly is a bit different than that.

While it enables more functionality and possibly better speeds, it also introduces complication to your system.


### When to build your own module

Using the WASM engine you can write your own sources, transforms, sinks, codecs in any language that compiles down to a
`.wasm` file. Vector can then compile this file down to an optimized, platform specific module which it can use. This
allows Vector to work with internal or uncommon protocols or services, support new language transforms, or just write a
special transform to do *exactly* what you want. With the WASM engine, Vector puts you in command.

There is a trade-off though! You'll need to embolden yourself, as WASM is a fledgling technology. When it first loads a
WASM file, Vector needs to spend some time building an optimized module before it can load it. You'll also be
responsible for the safety and correctness of your code, if your module crashes on an event, Vector will re-initialize
the module and try again on the next event. This should be modelled roughly off
[Erlang's OTP](https://erlang.org/doc/design_principles/sup_princ.html).

Here's some examples of when a WASM module is a good fit:

* You need to support a specific protobuf type.
* You have a source or sink which is not currently supported by Vector.
* You want to write a complex transform in your favorite language.

Here's some examples of where WASM probably isn't right for you:

* You only need basic functionality already offered by Vector.
* Your desired language doesn't support WASM as a compile target.
* You are using Vector on a non-Linux X86-64 bit compatible target. (Lucet, our Engine, only supports Linux at this
current time. Work on other operating systems is ongoing!)
* You need a battle-proven system, WASM support in Vector is young!


### How Vector modules work

Vector modules are `.wasm` files (WASI or not) exposing a predefined interface. We provide a `foreign_modules::guest`
module for Rust projects located in `lib/foreign-modules` of the source tree, and we will be providing bindings for
other languages as we see user demand.

Modules are sandboxed and can only interact with Vector over the Foreign Function Interface (FFI) which Vector exposes.
For each instance of a module, Vector holds some context. For example, when a transform module processes an event, the
context of the module contains an `Event` type. The guest can use calls like `get("foo")` and Vector perform the action
and return the result to the guest.

A guest module is not able to access the memory of the Vector instance, but Vector is free to modify the guest memory.
For example, behind the scenes of a `get` call, the Guest must allocate some memory for Vector to write the result into.

Due to the current semantics of the `vector::Event` type, passing data between WASM modules and the Vector host involves
serialization to JSON. We're investigating ways to speed this up.

* It may be possible to add a `repr(C)` flag to Event.
* It may be worthwhile to explore an `Event` refactor.
* Other serialization options may be much faster but less compatible.


### Initialization


Regardless of whether of not the module uses WASI, or has access to a language specific binding, it can interact with
Vector. Vector works by invoking the modules public interface. First, as soon as the module is loaded, `register` is
called once, globally, per module. In this call, the module can configure how Vector sees it and talks to it. During
this time, the module can do any one time setup it needs. (Eg. Opening a socket, a file).

If using the Vector guest API this can be done via:

```rust
use foreign_modules::{Registration, roles::Sink};

#[no_mangle]
pub extern "C" fn init() -> *mut Registration {
 &mut Registration::transform()
    .set_wasi(true) as *mut Registration
}
```

What happens next depends on which type of module it is.

### Module types

* `Transform`: Modules of this role can be used as a transform `type`.

  ```rust
  use foreign_modules::hostcall;

  // TODO: Add better FFI error handling!
  #[no_mangle]
  pub extern "C" fn process() -> usize {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
      0
  }
  ```

> **TODO:** Sources, sinks, and codecs have no POC or formal specification, below is a work in progress.

* `Source`: Modules of this role can be used as a source `type`.

  ```rust
  use foreign_modules::hostcall;

  #[no_mangle]
  // TODO: Add better FFI error handling!
  pub extern "C" fn start() {
      // TODO
  }
  ```

* `Sink`: Modules of this role can be used as a sink `type`.

  ```rust
  use foreign_modules::hostcall;

  #[no_mangle]
  // TODO: Add better FFI error handling!
  pub extern "C" fn process() {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
  }
  ```

* `Codec`:

  ```rust
  use foreign_modules::hostcall;
  // TODO: Add better FFI error handling!
  #[no_mangle]
  pub extern "C" fn process() {
      if let Some(value) = hostcall::get("field").unwrap() {
          hostcall::insert("field", value).unwrap();
      }
  }
  ```


### Your first module (Rust)

To create your first module, start with a working [Rust toolchain](https://rustup.rs/) add the `wasm32-wasi`
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
foreign_modules = { path = "/path/to/your/clone/of/vector/lib/foreign-modules"}
serde = { version = "1", features = ["derive"] }
```

Next, in your `lib.rs` file:

```rust
use foreign_modules::{Registration, hostcall};
use serde::{Serialize, Deserialize};

#[no_mangle]
pub extern "C" fn init() -> *mut Registration {
 &mut Registration::transform()
    .set_wasi(true) as *mut Registration
}

// TODO: Add better FFI error handling!
#[no_mangle]
pub extern "C" fn process() -> usize {
    let result = hostcall::get("message");

    match result.unwrap() {
        Some(value) => {
            hostcall::insert("echo", value);
            0
        },
        None => {
            0
        },
    }
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}
```

Now to build it:

```bash
cargo +nightly build --target wasm32-wasi --release
```

In your `target/wasm-wasi/release/` verify that the `echo.wasm` file exists. Next edit your Vector `config.toml`:

```toml
data_dir = "/var/lib/vector/"
dns_servers = []

[sources.source0]
max_length = 102400
type = "stdin"

[transforms.demo]
inputs = ["source0"]
type = "wasm"
module = "target/wasm32-wasi/release/echo.wasm"

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
$ vector test config.toml
Running config.toml tests
test config.toml: demo-tester ... passed
```

Congratulations, you're ready for a new frontier!

### Resources & templates

* [Rustinomicon](https://doc.rust-lang.org/nomicon/)
* [Lucet Docs](https://bytecodealliance.github.io/lucet/)
* [WASI](https://wasi.dev/)


## Sales Pitch

Integrating a flexible, practical, simple WASM module support means we can:

* Expose one common interface to any existing and future languages supported by WASM.
* Allow large-scale users to use custom encodings.
* Support and advertise a plugin system.
* Empower us to modularize Vector's components and increase maintainability.
* Share a common plugin format with Tremor.
* In the future, support something like AssemblyScript as a first class language.


## Drawbacks

WASM is still **very young**. We will be early adopters and will pay a high adoption price. Additionally, we may find
ourselves using unstable functionality of some crates, including a non-trivial amount of `unsafe`, which may lead to
unforseen consequences.

Our **binary size will bloom**. We currently (in unoptimized POC implementations) see our binary size grow by over 60
MB. The Lucet team is aware of this issue, and in some preliminary discussions this binary size increase is unexpected and
we expect it to improve. If this does not improve, we can consider adopting `wasmtime`, lucet's sister JIT implementation
which has a smaller binary footprint.

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
  * **Temporary Conclusion:** Held off on deciding. Some interest.
* As sources? This requires thinking about how we can do event sourcing.
  * **Temporary Conclusion:** Held off on deciding. Some interest.
* What about the idea of codecs being separate?
  * **Temporary Conclusion:** Held off on deciding. Some interest.
* This RFC makes some provisions for a future Event refactor, is it possible that this might happen?
  * **Temporary Conclusion:** Held off on deciding. Some interest.


### Consider supporting a blessed compiler

Our original discussions included the idea of a blessed compiler and a UX similar to our existing `lua` or `javascript`
transform. Indeed, our initial implementation should include just a transform, as it's by far the easiest place to
introduce this feature.

During investigation of the initial POC we determined that our most desirable "blessed" language would be
AssemblyScript, unfortunately, it's currently not able to run inside Lucet. Work is ongoing and we expect it to be
possible soon.

**Temporary Conclusion:** AssemblyScript and other languages we evaluated did not provide a way to package themselves as
WASM modules, making it hard to integrate with them. We are planning to revisit this at a later date when the ecosystem
is more mature.


### Consider API tradeoffs

We should consider if we are satisfied with the idea of hostcalls being used for `get` and other API. We could also let
the host call the Guest allocate function and then pass it a C String pointer to let it work on. This, however, requires
serializing and deserializing the entire Event each time, which is a huge performance bottleneck.

We could also consider adopting a new strategy for our `event` type. However, that work is outside the scope of this
RFC. The current structure of the `Event` type is already discussed in
[#1891](https://github.com/timberio/vector/issues/1891).

We should also consider if we want to change how we handle codecs, since it is likely that WASM module use cases will
include wanting to add codec support to already existing sources.

**Temporary Conclusion:** We will adopt hostcalls for the time being to allow for a minimal POC. We would like to
investigate these choices lat er, and we will make a decision before stabilizing WASM modules.


### Consider Packaging Approach

We will need to expose some Vector guest API to users. This will require ABI compatibility with Vector, so it makes
sense to keep it in the same repository (and maybe the same crate?) as Vector. We can consider either making Vector able
to be built by WASI targets (more feature flags) or using a workspace and having the guest API as a workspace member we
publish.

A couple good questions to consider:

* Do we one day want to support a cross platform `cargo install vector` option for installing a Vector binary?
* Should a user import `vector::...` to use the Vector guest API in their Rust module?
* Vector's internal API is largely undocumented and kind of a mess. Should we hide/clean the irrelevant stuff?

**Conclusion:** We are including a workspace member `foreign_modules` in the `lib/foreign-modules` directory.
This may be renamed and/or published in the future.


### Consider observability

How can we let users see what's happening in WASM modules? Can we use tracing somehow? Lucet supports tracing, perhaps
we could hook in somehow?

**Conclusion:** Preliminary results show we may be able to allow guests to write with the current tracing subscriber.


### Consider Platform Support

There is ongoing work to support more platforms in Lucet, here are how things stand today:


Platform support:

* [x] Linux (x86_64)
* [ ] Linux (ARMv7) (Likely never)
* [ ] Linux (Aarch64) (Probably coming, ARM contributing)
* [ ] Mac (x86_64)
  * https://github.com/bytecodealliance/lucet/pull/437
* [ ] Windows (x86_64)
  * https://github.com/bytecodealliance/wasmtime/pull/1216
  * https://github.com/bytecodealliance/lucet/issues/442
  * https://github.com/bytecodealliance/lucet/pull/437
* [ ] FreeBSD (x86_64)
  * https://github.com/bytecodealliance/lucet/pull/419
  * https://github.com/bytecodealliance/lucet/pull/437

**Temporary Conclusion:** The Lucet team desires to support other platforms in the near future. Since the lion's share
of Vector usage is on X86_64 Linux, and this platform is already supported, we decided to adopt Lucet for now. We could
consider adopting `wasmtime` in the future if lucet does not eventually reach it in terms of platform parity.


## Plan of attack

Incremental steps that execute this change.

* v0.1: (Done) This draft forms the groundwork for this RFC, and demos a POC of how a theoretical user could use this
  for a protobuf decoding transform, and permitting the first `wasm` transform test to pass.
* v0.2: The POC of the initial transform has it's v1
* v0.3: We have benchmarks of the initial transform POC.
* v0.4: Compile artifact caching is implemented, so Vector doesn't unnecessarily recompile wasm modules.
* v0.5: Guest API expanded to support majority of Event API.
* v0.6: We talk to a couple known users who would be interested in this feature and let them test it out.
* v0.7: This RFC is amended to include Sinks and Sources information.
* v0.8: This RFC is amended to include information about possible codec changes.
* v0.9: Source and Sink implementation POCs made.
* v0.10: Final APIs specced and tested. Source, Sink, Transform POCs are in tree and running as tests/benches.
* v1.0: This RFC is complete and we announce with the above guide.

Note: This can be filled out during the review process.
