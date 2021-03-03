# vrl-wasm

This directory houses the assets for compiling a [WebAssembly][wasm] binary for [Vector Remap Language][vrl].

## Prerequisites

* Install [wasm-pack]
* Install the `wasm32-unknown-unknown` target for Rust

## Build

To build a binary runnable in the browser:

```bash
wasm-pack build --target web
```

That outputs a variety of files in `pkg`.

## Usage

This Wasm module offers just one function, `resolve`, which takes a single JSON object with two fields:

* `program` is the VRL input program (string)
* `event` is the VRL event that you want to transform (JSON object)

Here's an example usage of the binary from JavaScript:

```javascript
// Assuming you've already compiled into ./pkg:
import { default as wasm, resolve } from "./pkg/vrl_wasm.js";

wasm().then((module) => {
  // An example VRL program
  const program = `
    . |= parse_key_value!(string!(.message))
    del(.message)
    .timestamp = format_timestamp!(to_timestamp!(.timestamp), format: "%+")
    .id = uuid_v4()
  `;

  // An example VRL event
  const event = {
    message: "foo=bar bar=baz",
    timestamp: "2021-03-02T18:51:01.513Z"
  };

  // The full input object
  const input = {
    program: program,
    event: event
  };

  // Get a result back
  let result = resolve(input);
  let json = JSON.stringify(result);

  // Log the JSON result
  console.log(result);
}
```

The [`index.html`][html] file provides a full example.

## Serve

Keep in mind that you can't use Wasm in the browser by just opening `index.html`. Instead, you need
to run a web server serving up this directory. Python provides an easy way to do that:

```bash
python3 -m http.server 8000
```

Navigate to http://localhost:8000 and the console should print JSON that looks like this (though not formatted):

```json
{
  "output": "1323aac3-c15a-4763-b417-d823ea7df10c", // UUID varies
  "result": {
    "bar": "baz",
    "foo": "bar",
    "id": "1323aac3-c15a-4763-b417-d823ea7df10c", // UUID varies
    "message": "foo=bar bar=baz",
    "timestamp": "2021-03-02T18:51:01.513+00:00"
  }
}
```

[html]: ./index.html
[vrl]: https://vrl.dev
[wasm]: https://webassembly.org
[wasm-pack]: https://github.com/rustwasm/wasm-pack
