# vrl-wasm

This directory houses the assets for compiling a [WebAssembly][wasm] binary for [Vector Remap Language][vrl].

## Prerequisites

* Install [wasm-pack]
* Install the `wasm32-unknown-unknown` target for Rust

To run the web app, install [Yarn] and then:

```bash
yarn
```

## Build

To build a Wasm binary runnable in the browser:

```bash
wasm-pack build --target web
```

That outputs a variety of files in `pkg`.

## Using the module from JavaScript

This Wasm module offers just one function, `resolve`, which takes a single JSON object with two fields:

* `program` is the VRL input program (string)
* `event` is the VRL event that you want to transform (JSON object)

Here's an example usage of the binary from JavaScript:

```javascript
import { resolve } from 'vrl-wasm.js';

const program = `.name = "Lee Benson"`;

const event = {
  game: "GraphQL"
};

const input = {
  program: program,
  event: event
};

const res = resolve(input);

if (res.error) {
  console.log(`Something went wrong: ${JSON.stringify(res.error)}`);
} else if (res.result) {
  console.log(`Output: ${res.output}`);
  console.log(`New event: ${JSON.stringify(res.result)})
}
```

The [`index.js`][./assets/index.js] file provides a full example.

## Serving the app

The web app is built using the [Parcel] bundler. The web app uses this initial event...

```json
{
  "message": "bar=baz foo=bar",
  "timestamp": "2021-03-02T18:51:01.513+00:00"
}
```

...and runs this initial program:

```ruby
. |= parse_key_value!(string!(.message))
del(.message)
.id = uuid_v4()
```

To run the app locally:

```bash
yarn run start
```

This automatically opens the browser to http://localhost:1234. The **Event** field should show an
event like this:

```json
{
  "bar": "baz",
  "foo": "bar",
  "id": "1323aac3-c15a-4763-b417-d823ea7df10c", // UUID varies
  "timestamp": "2021-03-02T18:51:01.513+00:00" // Timestamp varies
}
```

The **Output** field should be a UUID, as `.id = uuid_v4()` is the last line in the initial program.

[html]: ./index.html
[parcel]: https://parceljs.org
[vrl]: https://vrl.dev
[wasm]: https://webassembly.org
[wasm-pack]: https://github.com/rustwasm/wasm-pack
