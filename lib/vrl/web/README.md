# Vector Remap Language Web

This directory houses the assets used to build the VRL web app. The [Trunk] build tool compiles
various assets—[Sass](./sass), [`index.html`](./index.html), and [Rust sources](./src)—into a single
[WebAssembly][wasm]-powered site.

The app uses the [Yew] front-end framework for Rust and the [Bulma] CSS framework.

## Prerequisites

* [npm]
* [Trunk]

## Set

Install npm assets:

```bash
npm i
```

## Build

```bash
trunk build
```

## Serve

```bash
trunk serve
```

[bulma]: https://bulma.io
[npm]: https://npmjs.org
[trunk]: https://github.com/thedodd/trunk
[wasm]: https://webassembly.org
[yew]: https://yew.rs
