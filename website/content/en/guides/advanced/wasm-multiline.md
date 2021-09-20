---
title: Architecting Wasm Plugins
description: Build a Wasm plugin from scratch
authors: ["hoverbear"]
domain: transforms
transforms: ["wasm"]
weight: 1
noindex: true
tags: ["webassembly", "wasm", "multiline", "multi-line", "advanced", "guides", "guide"]
---

{{< warning >}}
[Vector `wasm` support was removed in
v0.17.0](https://github.com/vectordotdev/vector/issues/8036). This guide remains for
posterity.

It can be used with Vector versions prior to its removal.
{{< /warning >}}

{{< requirement title="Pre-requisites" >}}
* You understand the [basic Vector concepts][docs.about.concepts] and understand [how to set up a pipeline][docs.setup.quickstart].
* You must be using a Linux system (or WSL2 for Windows users) for WASM related work right now.
* You read the [Hello, WASM World][wasm_guide] guide and feel comfortable with the topics it discussed.
* You've reviewed the [Unit Testing][unit_tests] guide.

[docs.about.concepts]: /docs/about/concepts
[docs.setup.quickstart]: /docs/setup/quickstart
[unit_tests]: /guides/level-up/unit-testing
[wasm_guide]: /guides/advanced/wasm-hello
{{< /requirement >}}

In the [Hello, Wasm world][wasm_guide] guide we learned how to get started making our first WASM plugin. In this guide, we'll learn how to go farther with that skill by building a plugin that merges one long message split over multiple lines into a single log message.

{{< success title="Goals" >}}
In this guide you'll learn how to:

* Maintain state between events.
* Merge messages and perform string interpolation.
* Build a native Rust test suite.
* Build a Vector test suite.
{{< /success >}}

## Our mock ticket

Our pretend boss assigned us a new issue! It says we need to turn these 4 messages:

```json title="input.ndjson"
              { "vector": "one tool for all your observability needs",
  "version": "0.10.0" }
{ "vic":
                "the flying squirrel" }
```

Into these two:

```json
{ "vector": "one tool for all your observability needs", "version": "0.10.0" }
{ "vic": "the flying squirrel" }
```

Such drudgery 🙄! Oh well, some input feeding us junk *does* mean we get to write Rust!

In order to make it work, we'll need to:

* Accept *n* messages as input, storing them concatenated together inside some state.
* Determine after each input if the new state is parseable JSON.
* If the state is valid JSON, output it.

This sounds fun, so let's get started!

## Building a workspace

Let's start from scratch:

```bash
PLUGIN_NAME=banana
cargo init --lib ${PLUGIN_NAME}
cd ${PLUGIN_NAME}
```

Next, add the following content, setting the crate up as a `cdylib` and adding some important libraries:

```toml title="Cargo.toml"
[lib]
crate-type = ["cdylib"]

[dependencies]
vector-wasm = { version = "0.1", git = "https://github.com/vectordotdev/vector/"}
serde_json = "1.0"
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
```

{{< info >}}
For now, we use `serde_json` for (de)serializing FFI messaging. This will change in a future version.
{{< /info >}}


Now, scaffold out the minimal structure of the crate, this one does nothing, it just passes data on:

```rust title="src/main.rs"
#![deny(improper_ctypes)]
use std::convert::TryInto;
use vector_wasm::{hostcall, Registration, Role};

/// Perform one time initialization and registration.
///
/// During this time Vector and the plugin can validate that they can indeed work together,
/// do any one-time initialization, or validate configuration settings.
///
/// It's required that the plugin call [`vector_wasm::Registration::register`] before returning.
#[no_mangle]
pub extern "C" fn init() {
    // Vector provides you with a [`vector_wasm::WasmModuleConfig`] to validate for yourself.
    let config = hostcall::config().unwrap();
    assert_eq!(config.role, Role::Transform);

    // Finally, pass Vector a [`vector_wasm::Registration`]
    Registration::transform().register().unwrap();
}

/// Process data starting from a given point in memory to another point.
///
/// It's not necessary for the plugin to actually read, or parse this data.
///
/// Call [`vector_wasm::hostcall::emit`] to emit a message out.
///
/// # Returns
///
/// This function should return the number of emitted messages.
#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    // Vector allocates a chunk of memory through the hostcall interface.
    // You can view the data as a slice of bytes.
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };
    hostcall::emit(data).unwrap();

    // Hint to Vector how many events you emitted.
    1
}

/// Perform one-time optional shutdown events.
///
/// **Note:** There is no guarantee this function will be called before shutdown,
/// as we may be forcibly killed.
#[no_mangle]
pub extern "C" fn shutdown() {}
```

Let's review what we've written quickly:

* We defined 3 functions, `init`, `process`, and `shutdown` (with `#[no_mangle]`). These are the same as the `lua` v2 transform.
* In the `init` function, we called `Registration::register`, registering with the host.
* In the `process` function, we called `hostcall::emit` to emit events farther through the pipeline.
* We called some `unsafe` code that read from `data` with `length` as a byte array.

Before going farther, make sure it works (natively and in WASM!):

```bash
cargo build
cargo build --target wasm32-wasi
```

## Writing Code

In our example we don't need to concern ourselves with the `init()` and `shutdown()` functions. Those are already set up
for us.

{{< info >}}
In the [*Hello, WASM World*][wasm_guide] we used a test plugin called `add_fields` from Vector that did some
set up in `init`.

[wasm_guide]: /guides/advanced/wasm-hello
{{< /info >}}

## `process`

Each time the `process` function is called, a new event is arriving at the transform. In the `process` function, we get
a pointer to some data, and the length of it. From there, we can decide what we want to do.

Since Vector always gives us a correctly structured JSON representation of the `Event`, we first need to parse that, and
extract our partial message.

We can take the pointer and length as a slice of `u8` values, we can then parse that into a `serde_json::Value`:

```rust
let data = unsafe {
    std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
        .as_mut()
        .unwrap()
};
let value: serde_json::Value = serde_json::de::from_slice(data).unwrap();
```

From this event, we need to take out the message, containing the partially complete JSON:

```rust
let message_field = value.get("message").and_then(serde_json::Value::as_str)
    .unwrap_or(Default::default()); // Fall back to empty.
```

Next, we'll need to introduce some mutable state. We can use a `std::sync::Mutex` and a `once_cell::sync::Lazy` for this.

**In your global scope at the top of the file, add these.**

```rust
// A value which is initialized on the first access.
use once_cell::sync::Lazy;
// A mutual exclusion primitive useful for protecting shared data
use std::sync::Mutex;

// The working state of the string which represents a partial JSON.
static STATE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
```

We'll be able to lock, and then mutate, this `STATE`, gradually building it up.

Back over in the `process` function, let's sketch out an API that's semantically meaningful and testable.

```rust
match transform(&mut *STATE.lock().unwrap(), message_field) {
    Ok(Some(value)) => {
        let value_string = value.to_string();
        hostcall::emit(value_string.into_bytes()).unwrap();
        1
    },
    Ok(None) => 0,
    Err(e) => {
        // This is an unexpected error. The state will be reset.
        hostcall::raise(e).unwrap();
        0
    },
}
```

This means we can go write all our plugin-specific code in a function called `transform` that takes a mutable borrow
of the state, and an immutable view of the new arrival. If the `transform` function returns a value with no error, we
emit it out of Vector. If it returns nothing, we presume it was only a partial of a JSON, and do nothing. If we get an
error, we just pass it up.

Now it's time to write our main logic!

```rust
fn transform(state: &mut String, arrival: impl AsRef<str>) -> Result<Option<serde_json::Value>, Error> {
    // Add the new arrival on.
    state.push_str(&mut arrival.as_ref());

    // Try to read from it using a "reader" non-destructively.
    let mut working_state = state.clone();
    let de = serde_json::Deserializer::from_str(&working_state);
    let mut de_stream = de.into_iter::<serde_json::Value>();

    let output = de_stream.next();
    match output {
        Some(Ok(value)) if value.is_object() => {
            let offset = de_stream.byte_offset();

            let new_state = working_state.split_off(offset);
            *state = new_state;
            Ok(Some(value))
        },
        Some(Ok(value)) => {
            // This is an unexpected error. The most we can do is report it and clear our state.
            state.clear();
            Err(anyhow::anyhow!("Was provided {}, not an object", value))
        }
        Some(Err(e)) if e.is_eof() => {
            // Not an error, keep going!
            Ok(None)
        },
        None => {
            // Not an error, keep going!
            Ok(None)
        }
        Some(Err(e)) => {
            // This is an unexpected error. The most we can do is report it and clear our state.
            state.clear();
            Err(e.into())
        },
    }
}
```

Since we cleverly created a new function which returns a `Result` we can use the `?` operator inside, allowing us to
easily bubble up errors.

We also structured our function for **testability**. Since the `transform` function doesn't have side effects
(it takes in a mutable state, instead of directly mutating `STATE`) we can more easily test it for correctness.

At this point you should try `cargo check` or `cargo build` to see if our code builds!

{{< info >}}
There's a full, buildable copy of the code at the bottom of this page.
{{< /info >}}

### Testing (Rust)

For our module, we're actually going to do two phases of testing.

You might have noticed that in our effort to create a partial JSON parser Wasm plugin we also inadvertently created a
native one.

{{< warning >}}
*Whoops.* Guess we'll just have to use it for testing. 🙄
{{< /warning >}}

Let's create both a test module, and our first test like so:

```rust
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single() -> Result<(), Error> {
        let expected = serde_json::json!({
            "foo": "bar",
        });
        let mut working_buffer = String::from("");

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "foo": "bar" }"#,
        );

        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_some());
        let out_json = out_option.unwrap();
        assert_eq!(out_json, expected);
        Ok(())
    }
}
```

Try to write a few more!

These tests are excellent for making sure that our `transform` works as expected, but you've probably noticed
that there's some code we're not testing, and we're not really testing it inside Vector, either.

Let's solve that next! First though, we need to build our `wasm` plugin.

```bash
cargo build --release --target wasm32-wasi
```

Our new module is now located at `target/wasm32-wasi/release/${PLUGIN_NAME}.wasm`.

### Testing (Vector)

As you may recall from the [Unit Testing Guide][unit_test]s, Vector supports testing configurations right out of the box.
We'll use this to test our new Wasm plugin.

{{< info >}}
Review the [Hello, Wasm world!][wasm_guide] guide if you need a refresher on how to get a `wasm` compatible Vector during the experimental phase.

[wasm_guide]: /guides/advanced/wasm-hello
{{< /info >}}

Using your Wasm-capable Vector, create a configuration, adding the plugin name as appropriate:

```toml title="${PLUGIN_NAME}.toml"
data_dir = "/var/lib/vector/"

[sources.source0]
  max_length = 102400
  type = "stdin"

[transforms.transform0]
  inputs = ["source0"]
  type = "wasm"
  module = "target/wasm32-wasi/release/${PLUGIN_NAME}.wasm"
  artifact_cache = "tmp"

[sinks.sink0]
  healthcheck = true
  inputs = ["transform0"]
  type = "console"
  encoding = "json"
  buffer.type = "memory"
  buffer.max_events = 500
  buffer.when_full = "block"
```

Then, using your Wasm-capable Vector build take it for a test drive:

```bash
ana@autonoma:~/git/vectordotdev/banana$ ../vector/target/release/vector --config banana.toml
Aug 25 10:29:19.705  INFO vector: Log level "info" is enabled.
Aug 25 10:29:19.706  INFO vector: Loading configs. path=["banana.toml"]
Aug 25 10:29:19.708  INFO vector: Vector is starting. version="0.11.0" git_version="v0.9.0-530-g1b9eadd" released="Mon, 17 Aug 2020 20:48:21 +0000" arch="x86_64"
Aug 25 10:29:19.708  INFO vector::sources::stdin: Capturing STDIN.
Aug 25 10:29:19.708  INFO vector::internal_events::wasm::compilation: WASM Compilation via `lucet` state="beginning" role="transform"
Aug 25 10:29:19.708  INFO vector::internal_events::wasm::compilation: WASM Compilation via `lucet` state="cached" role="transform"
Aug 25 10:29:19.708  INFO vector::topology: Running healthchecks.
Aug 25 10:29:19.708  INFO vector::topology: Starting source "source0"
Aug 25 10:29:19.708  INFO vector::topology: Starting transform "transform0"
Aug 25 10:29:19.708  INFO vector::topology: Starting sink "sink0"
Aug 25 10:29:19.708  INFO vector::topology::builder: Healthcheck: Passed.
{ "foo": "bar" }
{"foo":"bar"}
123
Aug 25 10:29:29.478 ERROR transform{id=transform0 type=wasm}: vector::wasm: WASM plugin errored: Was provided 123, not an object
{ "foo":
"bar" }
{"foo":"bar"}
Aug 25 10:29:38.135  INFO vector::shutdown: All sources have finished.
Aug 25 10:29:38.135  INFO source{id=source0 type=stdin}: vector::sources::stdin: finished sending
Aug 25 10:29:38.135  INFO vector: Shutting down.
```

Good enough to start.

Before you go and deploy it, give it a more thorough testing, try using a `file/wasm/file` pipeline and checking
to make sure the results are your expectation. Try to also think of some potential issues!

When you have some ideas of what to test, you can add a new behavior test to Vector:

```toml
[[tests]]
  name = "test"

  [[tests.inputs]]
    insert_at = "transform0"
    type = "log"
    log_fields.message = "{ \"foo\":"

  [[tests.inputs]]
    insert_at = "transform0"
    type = "log"
    log_fields.message = "\"bar\" }"

  [[tests.outputs]]
    extract_from = "transform0"

  [[tests.outputs.conditions]]
    "foo.equals" = "bar"
```

Running the test:

```bash
ana@autonoma:~/git/vectordotdev/banana$ ../vector/target/release/vector test banana.toml
Aug 25 10:49:42.370  INFO vector: Log level "info" is enabled.
Running banana.toml tests
Aug 25 10:49:42.374  INFO vector::internal_events::wasm::compilation: WASM Compilation via `lucet` state="beginning" role="transform"
Aug 25 10:49:42.374  INFO vector::internal_events::wasm::compilation: WASM Compilation via `lucet` state="cached" role="transform"
test banana.toml: test ... passed
```

Perfect! Great job!

## Next steps

You may have noticed, the plugin we made does not persist `timestamp` or `host` keys. Our plugin also doesn't support
any options, such as changing the key to parse from, or the key to save to.

These would be great next steps, but you are free to soar as you please! 🐦

## Worked example

```rust title="src/main.rs"
#![deny(improper_ctypes)]
use std::{convert::TryInto, sync::Mutex};
use once_cell::sync::Lazy;
use vector_wasm::{hostcall, Registration, Role};
use anyhow::Error;

static STATE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

/// Perform one time initialization and registration.
///
/// During this time Vector and the plugin can validate that they can indeed work together,
/// do any one-time initialization, or validate configuration settings.
///
/// It's required that the plugin call [`vector_wasm::Registration::register`] before returning.
#[no_mangle]
pub extern "C" fn init() {
    // Vector provides you with a [`vector_wasm::WasmModuleConfig`] to validate for yourself.
    let config = hostcall::config().unwrap();
    assert_eq!(config.role, Role::Transform);

    // Finally, pass Vector a [`vector_wasm::Registration`]
    Registration::transform().register().unwrap();
}

/// Process data starting from a given point in memory to another point.
///
/// It's not necessary for the plugin to actually read, or parse this data.
///
/// Call [`vector_wasm::hostcall::emit`] to emit a message out.
///
/// # Returns
///
/// This function should return the number of emitted messages.
#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    // Vector allocates a chunk of memory through the hostcall interface.
    // You can view the data as a slice of bytes.
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };
    let value: serde_json::Value = serde_json::de::from_slice(data).unwrap();

    let message_field = value.get("message").and_then(serde_json::Value::as_str)
        .unwrap_or(Default::default()); // Fall back to empty.
    match transform(&mut *STATE.lock().unwrap(), message_field) {
        Ok(Some(value)) => {
            let value_string = value.to_string();
            hostcall::emit(value_string.into_bytes()).unwrap();
            1
        },
        Ok(None) => 0,
        Err(e) => {
            // This is an unexpected error. The most we can do is report it and clear our state.
            hostcall::raise(e).unwrap();
            0
        },
    }
}

/// Perform one-time optional shutdown events.
///
/// **Note:** There is no guarantee this function will be called before shutdown,
/// as we may be forcibly killed.
#[no_mangle]
pub extern "C" fn shutdown() {}

fn transform(state: &mut String, arrival: impl AsRef<str>) -> Result<Option<serde_json::Value>, Error> {
    // Add the new arrival on.
    state.push_str(&mut arrival.as_ref());

    // Try to read from it using a "reader" non-destructively.
    let mut working_state = state.clone();
    let de = serde_json::Deserializer::from_str(&working_state);
    let mut de_stream = de.into_iter::<serde_json::Value>();

    let output = de_stream.next();
    match output {
        Some(Ok(value)) if value.is_object() => {
            let offset = de_stream.byte_offset();

            let new_state = working_state.split_off(offset);
            *state = new_state;
            Ok(Some(value))
        },
        Some(Ok(value)) => {
            // This is an unexpected error. The most we can do is report it and clear our state.
            state.clear();
            Err(anyhow::anyhow!("Was provided {}, not an object", value))
        }
        Some(Err(e)) if e.is_eof() => {
            // Not an error, keep going!
            Ok(None)
        },
        None => {
            // Not an error, keep going!
            Ok(None)
        }
        Some(Err(e)) => {
            // This is an unexpected error. The most we can do is report it and clear our state.
            state.clear();
            Err(e.into())
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single() -> Result<(), Error> {
        let expected = serde_json::json!({
            "foo": "bar",
        });
        let mut working_buffer = String::from("");

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "foo": "bar" }"#,
        );

        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_some());
        let out_json = out_option.unwrap();
        assert_eq!(out_json, expected);
        Ok(())
    }

    #[test]
    fn errors() -> Result<(), Error> {
        let mut working_buffer = String::from("");

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "foo" }"#,
        );
        assert!(out_result.is_err());
        Ok(())
    }

    #[test]
    fn double() -> Result<(), Error> {
        let expected = serde_json::json!({
            "foo": "bar",
        });
        let mut working_buffer = String::from("");

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "foo":"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_none());

        let out_result = transform(
            &mut working_buffer,
            &r#""bar" }"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_some());
        let out_json = out_option.unwrap();
        assert_eq!(out_json, expected);

        Ok(())
    }

    #[test]
    fn multiple_expected() -> Result<(), Error> {
        let expected = serde_json::json!({
            "foo": "bar",
        });
        let mut working_buffer = String::from("");

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "foo":"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_none());

        let out_result = transform(
            &mut working_buffer,
            &r#""bar" }"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_some());
        let out_json = out_option.unwrap();
        assert_eq!(out_json, expected);

        let expected = serde_json::json!({
            "baz": "bean",
        });

        let out_result = transform(
            &mut working_buffer,
            &r#"{ "baz":"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_none());

        let out_result = transform(
            &mut working_buffer,
            &r#""bean" }"#,
        );
        assert!(out_result.is_ok());
        let out_option = out_result?;
        assert!(out_option.is_some());
        let out_json = out_option.unwrap();
        assert_eq!(out_json, expected);

        Ok(())
    }
}
```

[unit_tests]: /guides/advanced/unit-testing
[wasm_guide]: /guides/advanced/wasm-hello
