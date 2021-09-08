---
title: Hello, Wasm world
description: Write your first Wasm plugin for Vector
authors: ["hoverbear"]
domain: transforms
transforms: ["wasm"]
weight: 3
noindex: true
tags: ["webassembly", "wasm", "transform", "advanced", "guides", "guide"]
---

{{< warning >}}
[Vector `wasm` support was removed in
v0.17.0](https://github.com/timberio/vector/issues/8036). This guide remains for
posterity.

It can be used with Vector versions prior to its removal.
{{< /warning >}}

{{< requirement title="Pre-requisites" >}}
* You understand the [basic Vector concepts][docs.about.concepts] and understand [how to set up a pipeline][docs.setup.quickstart].
* You must be using a Linux system (or WSL2 for Windows users) for WASM related work right now.

[docs.about.concepts]: /docs/about/concepts
[docs.setup.quickstart]: /docs/setup/quickstart
{{< /requirement >}}

Vector supports a robust, language-agnostic WebAssembly runtime with WASI support. While support
matures, we encourage you to try out the transform and consider how it might fit into your pipelines.

In this guide you'll:

* Build your own custom rolled Vector release.
* Develop your first Vector plugin.
* Review how to test and benchmark the plugin, as well as Vector.

First, let's talk a bit about what Vector plugins are! Just want to dig in? [**Fastpath to Hacking!**](#limitations)

## Why does Vector need plugins?

We had a few reasons to add plugin support!

* **Build Complexity is first.** Vector connects sources to sinks, with some transformation magic in between. The more
  **stuff** you support, the more Vector grows in size. Each component we add brings with it a new set of dependencies,
  increasing things like binary size, testing time, and linkage which need to be made and optimized.

  Eventually, Vector will grow large enough that some folks won't be able to contribute to it, their computers won't be
  able to handle it. What a nightmare scenario 😔. *Fear not. Plugins can help us.* They allow us to decompose Vector
  components into small units that don't need to be part of the main linkage.

* **Language capabilities and limitations is up next.** Vector is built on Rust. It's a static, systems language which
  relies on ahead-of-time compilation. Normally, this is a most excellent decision, but in some niche cases it creates
  big problems! *Protobufs* are a good example of this problem. See, most fast protobuf libraries are *generated* by a
  tool called `protoc` or something like `prost`, a protobuf crate that generates code at build time.

    This is not something Vector can do without adding a **lot** of dependencies. Normally in a situation like this, the
    best solution would be to work with the upstream ecosystem to come up with efficient runtime solutions. We tried this,
    and there was a path forward here, for protobufs. But we knew there were other formats like Avro or Cap'n Proto.
    This is a problem that will keep repeating. Plugins let our users go ahead and roll their own custom deserializers
    for any protocol they want, and we can build tooling to help them do that in a nice way.

* **Lastly, Lua, DSLs, and other configuration methods are so fun to use!** We found users really enjoy the feeling of
  using a language they're familiar with to use Vector. Our Lua transform is very popular, and we often get
  requests for other languages like Javascript.

    WASM lets us solve this. By exposing a WASM interface, we can support any language that compiles to WASM. We hope this
    will let us provide a better, more familiar experience to users who like to write their own scripts.

## What's a WASM anyhow?

WebAssembly (or WASM) is an execution format.

Compilers like `rustc` can output code into WebAssembly format (either binary `.wasm` or UTF-8 `.wat`). These are just
like `.so` or `.dll` files you may have seen in your filesystem.

If you've specified a WASM module in your Vector config, Vector will go ahead and load it, then optimize the code for
the exact machine it's running on. Vector then executes that code, and communicates with it through a fast, C-like
foreign-function interface.

WASM plugins act as a "stack machine" and Vector sandboxes them with a limited set of capabilities and resources.
A plugin can't go blow out the heap, read your private keys, or send your data to the NSA.

It's an emerging format, and only has risen to usability in recent years. Our addition of WASM support is still limited
not only due to our implementation being quite new, but WASM itself being quite new.

## Why are Vector plugins WASM based?

WASM is an **operating system and architecture agnostic format**. Once runtime support matures, users of WASM plugins
will be able to use the same plugin on Linux, Windows, Mac, or any other platform our runtimes support.

Since many languages can target WASM (and more emerge all the time!) we can **avoid having to package a menagerie of
language runtimes** while still providing users with support for their favorite languages. In addition, users should
find it unchallenging to add support for new languages which already compile to WASM. In most cases, it's just providing
a nice API over the hostcall interface.

Vector's WASM plugins run in a sandbox, we can closely **control the runtime resources of plugins and perform auditing
on them**. We can closely limit the amount of memory or time available for a plugin to run, inspect its memory during
runtime, or even audit/test plugins for faults or issues before running it.

While all that sounds great, it doesn't mean anything if Vector components runs slower in plugins! The good news here is
that the cost of a WASM call is typically under 10ns, so unless you're truly pushing Vector to its limits already, you
shouldn't notice a dip in performance.

## Cook your environment and kick the wheels

In order to develop and test a WASM plugin for Vector, you'll need to do two things:

* Build Vector with WASM support.
* Run the existing Vector WASM tests.

Let's do that now:

First, [consult the CONTRIBUTING.md guide on how to set up your development environment](https://github.com/timberio/vector/blob/master/CONTRIBUTING.md#development), or bootstrap an Ubuntu 20.04 host/VM/Container with:

```bash
apt install build-essentials git cmake llvm lld clang libssl-dev protobuf-compiler
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

With your machine provisioned, it's time to hack!

1. Add the `wasm32-wasi` target:

  ```bash
  rustup target add wasm32-wasi
  ```
1. Build Vector:  **(This will take a bit!)**

  ```bash
   cargo build --features wasm --release
   ```
1. Verify your handiwork, make sure the `wasm` component is loaded:

  ```bash
  ./target/release/vector list | grep wasm
  ```
1. Build the test WASM module Vector uses:

  ```bash
  make test-wasm-build-modules
  ```

At this point you should have:

* `target/release/vector`, a build of Vector with WASM support.
* `target/wasm32-wasi/release/*.wasm`, several test modules to use.

Let's take it for a spin! Make a config like so:

```toml title="vector.toml"
data_dir = "/var/lib/vector/"

[sources.source0]
max_length = 102400
type = "stdin"

[transforms.transform0]
inputs = ["source0"]
type = "wasm"
module = "target/wasm32-wasi/release/add_fields.wasm"
artifact_cache = "cache/"
options.new_field = "new_value"
options.new_field_2 = "new_value_2"

[sinks.sink0]
healthcheck = true
inputs = ["transform0"]
type = "console"
encoding = "json"
buffer.type = "memory"
buffer.max_events = 500
buffer.when_full = "block"
```

Then, `mkdir -p cache` to create a local directory for vector to store its optimized artifacts.

Now it's time for the best part! Run Vector and have a try, using `CTRL+C` to stop as needed:

```bash
ana@autonoma:~/git/timberio/vector$ ./target/release/vector --config test.toml
# ...
Aug 17 15:28:05.954  INFO vector::topology::builder: Healthcheck: Passed.
This is some input!
{"host":"autonoma","message":"This is some input!","new_field":"new_value","new_field_2":"new_value_2","source_type":"stdin","timestamp":"2020-08-17T22:28:11.183218406Z"}
^CAug 17 15:28:12.690  INFO vector: Shutting down.
Aug 17 15:28:12.690  INFO source{id=source0 type=stdin}: vector::sources::stdin: finished sending
```

Now lets dig into the module we tested, see how it works, then write your own!


## Hack on a Plugin

The easiest way to get started (for now!) is to copy an existing plugin inside Vector.


```bash
PLUGIN_NAME=banana
GITHUB_USER=hoverbear
VECTOR_DIR=${PWD}

mkdir -p ~/git/${GITHUB_USER}/
cp -r tests/data/wasm/add_fields ~/git/${GITHUB_USER}/${PLUGIN_NAME}
cd ~/git/${GITHUB_USER}/${PLUGIN_NAME}
sed -i 's@add_fields@'"${PLUGIN_NAME}"'@g' Cargo.toml
sed -i 's@\.\./\.\./\.\./\.\.@'"${VECTOR_DIR}"'@g' Cargo.toml
cargo build --target wasm32-wasi --release
```

At this point you should have:

* `target/wasm32-wasi/release/${PLUGIN_NAME}.wasm`, a WASM module you can load into Vector.

{{< info >}}
**Feature outlook**: We're investigating the best way to let users generate plugins from templates, as well as
integration with tools like `wapm`.
{{< /info >}}

Next, you should review the code in `src/lib.rs`, and start planning how to add your feature. You can browse other
examples in the `tests/data/wasm/` folder of Vector.

Some things to note:

* Accessing a chunk of guest memory as mutable slice can be done via:
  ```rust
  let data = unsafe {
    std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
        .as_mut()
        .unwrap()
  };
  ```
* `OnceCell` is useful for one-time initializers. Eg.
  ```rust
  static FIELDS: OnceCell<HashMap<String, Value>> = OnceCell::new();

  fn demo() {
      // ...
      FIELDS.set(config.options.into()).unwrap();
      // ...
  }
  ```
* Vector will reinitialize panicked modules, so panicing is safe so long as you're fine losing your working data.
* By default, the heap size is 10MB.

## Understanding the Current Limitations {#limitations}

Currently support for WASM is limited, we're investigating the best ways to support topics like:

* Variable numbers of output events. (Currently only 0 or 1 supported)
* Optimized stream support. (We use a fallback right now)
* Timeouts/deadlines/watchdogs. (We don't support this right now)
* Sockets and File descriptors. (We don't support this right now)
* Faster FFI communication. (We're using a fairly slow method right now, takes whole nanoseconds!)

Please let us know if you value one of these features and we can better prioritize it!

## Going Farther

You're already armed with the ability to make WASM plugins and build Vector. You have all the tools you need. Now is the
time to get excited, and start digging.

We'd love it if your energies ended up helping us improve Vector's core, share the plugins you make with others, or even
just show off the cool stuff you're doing to us!

Why not try to make a transform that removes fields?

[docs.about.concepts]: /docs/about/concepts/
[docs.setup.quickstart]: /docs/setup/quickstart/
