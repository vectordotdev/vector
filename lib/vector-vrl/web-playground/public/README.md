# VRL WASM Web Playground

This directory houses the exposed VRL function to WASM `run_vrl()` used to
power [Vector Remap Language Playground][vrl-playground], or **VRL Playground**
for short. Although there is already a [local REPL][vrl-repl] supported for
use within the terminal, this playground will support running VRL in the web
browser and test input via uploading event files, or specifying an event via
a text input field.

## Setup

To build the project we need to use `wasm-pack`. This compiles our Rust code
to WebAssembly which can then be used within the browser. Install it by running:

```shell
cargo install --version 0.10.3 wasm-pack
```

After installing `wasm-pack` we must compile our project by running:

```shell
wasm-pack build --target web --out-dir public
```

Notice the `public` directory was populated with `.wasm`, and `.js`,
files these will be used by our `index.html` to run the `run_vrl()`
function originally written in Rust.

For more information on Rust and WebAssembly please visit
[the mozilla docs][mozilla-wasm-rust-docs] or
[the Rust book wasm chapter][rust-book-wasm]

The `src/lib.rs` file is the entry point of the `web-playground` crate.
This file is necessary so we can use the `run_vrl()` function in the browser.
Notice our `index.html` imports the VRL wasm module from `./vrl_web_playground.js`
and sets the `window.run_vrl` function so that we can test VRL within
the web browser console.

To see this in action we host `index.html` locally, for example by running:

```shell
python3 -m http.server
```

Remember to be in the `public` directory where index.html is located for the
relative paths specified in `index.html` to work.
We should also be able to open the `index.html` file in chrome, or use Live Server
in VSCode to see `index.html` working.

## Support

Some functions of VRL are not supported or don't work as expected at the
moment due to WASM limitations with some Rust crates, in
the future we will modify the functions so that they are supported.

List of functions that aren't supported at the moment:

- `log()`
- `decrypt()`
- `encrypt()`
- `get_hostname()`
- `parse_groks()`
- `random_bytes()`
- `reverse_dns()`

Functions from VRL stdlib that are currently not supported can be found
with this [issue filter][vrl-wasm-unsupported-filter]

## Examples

### React

For now, you can use an old npm-published version of vrl-web-playground,
please note that this was done for testing purposes and in the future we
will likely release this automatically upon each version release of Vector,
probably under a different package name.

Use this dependency in `package.json`

```json
"dependencies": {
    "vrl-web-playground": "0.1.0"
}
```

Example import and usage in a React component

```javascript
import init, { run_vrl } from 'vrl-web-playground';

export function VectorExecuteButton() {
  let vrlInput = {};
  try {
    vrlInput = {
        program: '.something = "added by vrl!"\n.message = "modified by vrl!"',
        event: JSON.parse('{message: "log message here"}'),
    };
  } catch (error) {
        console.log('error parsing the event contents as JSON object');
  }

  return (
      <button
        onClick={() => {
            console.log("[DEBUG] Initializing WASM");
            init().then(() => {
                console.log("[DEBUG] WASM initialized");
                console.log("[DEBUG] Attempting to run vrl with input: ", vrlInput);

                let res = run_vrl(vrlInput);
                console.log("[DEBUG] run_vrl() output", res);
            });
        }}
      />
  );
}
```

### JavaScript

Example usage in vanilla JavaScript

```javascript
import init, { run_vrl } from "./vrl_web_playground.js";
let vrlInput = {};
try {
    vrlInput = {
        program: '.something = "added by vrl!"\n.message = "modified by vrl!"',
        event: JSON.parse('{message: "log message here"}'),
    };
} catch (error) {
    console.log('error parsing the event contents as JSON object');
}
init().then(() => {
    window.run_vrl = run_vrl;
    console.log(run_vrl(vrlInput));
});
```

[vector]: https://vector.dev
[vrl]: https://vrl.dev
[vrl-playground]: https://github.com/vectordotdev/vector/issues/14653
[mozilla-wasm-rust-docs]: https://developer.mozilla.org/en-US/docs/WebAssembly/Rust_to_wasm
[rust-book-wasm]: https://rustwasm.github.io/docs/book/
[vrl-repl]: https://github.com/vectordotdev/vector/tree/master/lib/vrl/cli
[vrl-wasm-unsupported-filter]: https://github.com/vectordotdev/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22vrl%3A+playground%22+wasm+compatible
