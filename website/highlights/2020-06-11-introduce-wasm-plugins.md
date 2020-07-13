---
last_modified_on: "2020-07-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Experimental WASM Plugins (Dev builds only)"
description: "Vector can be optionally built with an integrated WASM runtime"
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2006]
release: "0.10.0"
tags: ["type: new feature", "domain: transforms"]
---

import Alert from '@site/src/components/Alert';

If you're feeling particularly brave, you can now test drive the WASM support in Vector!

First, some big caveats:

* You'll need to **build your own version of Vector**.
  * Our runtime is a bit larger than we want, so we're working on slimming it down before we enable it by default!
* Only **transforms** are supported right now.
* The **API is unstable**.
  * We have plans to refactor absolutely all of it and make it much better. We intend to work with any new plugins that emerge to make sure their plans are achievable. Don't be a stranger. 😁
* Our WASM plugins don't yet have robust **watchdog timers** or **stream integration**. These are coming!

This is the begining of support for a **full WASM runtime for Vector**, which means in the future, we'll be able to easily **add support for languages** which compile to WASM, protocols with code generation like **Protobufs** or **Apache Arrow**, or meet the needs of **any** custom demand, without needing to change the core Vector.

We're just getting started with this feature, so please try it out and give us your constructive feedback!


## What's a WASM plugin look like?

<Alert type="warning">

Recall that this is experimental code, and the API may have changed by the time you read this!

In order to use this feature, Vector must be built with the `wasm` feature.

```bash
cargo build --release --features wasm
```

Then you can run Vector as `target/release/vector`.

</Alert>

In order to use a WASM plugin, you just specify it by path in your config:

```toml title="vector.toml"
[transforms.my_transform_id]
  type = "wasm" # required
  inputs = ["my-source-or-transform-id"] # required
  artifact_cache = "/etc/vector/artifacts" # required
  heap_max_size = 10485760 # optional, default
  module = "./modules/example.wasm" # required
```

Here's a sample from one of our proof of concepts:

```rust title="main.rs"
use serde_json::Value;
use std::collections::BTreeMap;
use vector_wasm::{hostcall, Registration};
use std::convert::TryInto;

pub use vector_wasm::interop::*; // Required!

#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };
    let mut event: BTreeMap<String, Value> = serde_json::from_slice(data).unwrap();
    event.insert("new_field".into(), "new_value".into());
    event.insert("new_field_2".into(), "new_value_2".into());
    hostcall::emit(serde_json::to_vec(&event).unwrap()).unwrap();
    1
}
```

This can then be turned into a `.wasm` file with `cargo build --release --target wasm32-wasi`, and loaded into Vector.


