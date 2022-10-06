# VRL WASM Web Playground

This directory houses the exposed VRL function to WASM `run_vrl()` used to
power [Vector Remap Language Playground][vrl-playground], or **VRL Playground**
for short. Although there is already a local REPL supported for use within the
terminal, this playground will support running VRL in the web browser and test
input via uploading event files, or specifying an event via a text input field.

## Setup

To build the project we need to use `wasm-pack`. This compiles our Rust code
to WebAssembly which can then be used within the browser. Install it by running:

```shell
cargo install 0.10.3 wasm-pack
```

After installing `wasm-pack` we must compile our project by running:

```shell
wasm-pack build --target web
```

Notice a `pkg` directory was created which contains `wasm_bg.wasm`, `wasm.js`,
these are the files that will be used by the web browser to run the
compiled Rust code.

For more information on Rust and WebAssembly please visit
[the mozilla docs][mozilla-wasm-rust-docs] or
[the Rust book wasm chapter][rust-book-wasm]

The `lib.rs` file is the entry point of the `web-playground` library.
This will make it so we can use the `run_vrl` function in the console.
Notice our `index.html` imports the VRL wasm module from `/pkg/` and
sets the `window.run_vrl` function so that we can test VRL within
the web browser console. To test out `index.html` we need to host it
locally, for example by running:

```shell
python3 -m http.server
```

Remember to be in the directory where index.html is located for it to function properly.

## Support

Some functions of VRL are not supported or don't function as expected at the
moment due to WASM compatibility with some dependencies that functions use, in
the future we will modify the functions so that it is supported.

List of functions that aren't supported at the moment:

- `log()`
- `decrypt()`
- `encrypt()`
- `get_hostname()`
- `parse_groks()`
- `random_bytes()`
- `reverse_dns()`

It is worth checking out this [issue](https://github.com/vectordotdev/vector/pull/6604/files)
which has some functions written in a way that would make it be WASM compatible.

[vector]: https://vector.dev
[vrl]: https://vrl.dev
[vrl-playground]: https://github.com/vectordotdev/vector/issues/14653
[mozilla-wasm-rust-docs]: https://developer.mozilla.org/en-US/docs/WebAssembly/Rust_to_wasm
[rust-book-wasm]: https://rustwasm.github.io/docs/book/
