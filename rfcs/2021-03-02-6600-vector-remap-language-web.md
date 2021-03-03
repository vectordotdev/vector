# RFC 6600 - 2021-03-03 - Vector Remap Language on the web

From the beginning of our work on [Vector Remap Language][VRL] (VRL), we've had discussions about
novel ways to make VRL approachable and even downright fun for users. Thus far, we've done some
pretty unique things, like providing a REPL for VRL. But we've also had plans, though fairly
unspecific ones, about making VRL available on the web in some form. This RFC seeks to lend some
concreteness to those discussions.

## Options

In this RFC, I'd like to discuss three possibities. The first two emerged from our internal
discussions; the third I've gathered from a survey of the Wasm ecosystem.

1. An in-browser [web app]/playground for experimenting with and learning VRL
2. A [web server] that can process VRL programs and events
3. A [published Wasm module] that can be used by third parties in whichever environment they wish

In general, I propose proceeding with #1 and #2 and achieving consensus around specifics. I propose
pursuing #3 only if VRL catches on in such a way that a published module would be beneficial.

### Interactive web app

An interactive VRL web app would be a browser-based app that provides an aesthetically pleasing and
UX-friendly experience for learning and experimenting with VRL. In its most basic form, it would
enable you to enter a VRL event and a VRL program into a text area and see what happens. But there
are other capabilities that we should consider:

* A step-by-step tutorial, like the one under way in PR [6184] that provides real exercises with
  right and wrong answers
* A built-in "library" of example events (JSON, Syslog, AWS formats, etc.)
* The ability to share URLs for specific VRL events and/or programs. This would likely require using
  a backend [web server] although it *may* be feasible to use URL hashes to achieve this.

#### Prior art

In PR [6590] I created a proof of concept for a VRL web app that uses the [Trunk] build tool and the
[Yew] front-end framework for Rust, which is inspired by [React.js][react] and enables you to write
web apps in pure Rust and without touching JavaScript. This POC does "work" but it creates an
unacceptably large Wasm binary (~15 MB), largely due to it containing both VRL (which is bulky in
itself) and *all* of the app's front-end logic. You can see a demo of that proof of concept
[here][vrl_web_app].

A much better solution, I believe, would be to create a web app that's mostly standard HTML, CSS,
and JavaScript and delegating the VRL processing logic to one of the following:

* A Wasm binary that only handles VRL logic
* The [web server] proposed below

### Web server

A VRL web server would be a simple HTTP server with one endpoint to which you
can `POST` the following:

* A VRL program (as a string)
* A VRL event (as JSON)
* (optional) a compiler state. This would require one change to VRL:
  `vrl::compiler::state::Compiler` would need to be serializable/deserializable
  to/from JSON.

Here's an example `POST` payload:

```json
{
  "program": "del(.foo)\n.bar = baz",
  "event": {
    "foo": "bar"
  },
  "state": {
    "variables": {
      "baz": 45.2
    }
  }
}
```

And the response:

```json
{
  "event": {
    "bar": 45.2
  }
}
```

A server of this kind would be quite germane to "serverless" environments like Lambda, as the logic
is fully stateless.

#### Implementation

The [Warp] framework would be a natural candidate for this. Warp is the only widely used Rust web
framework that both supports async and uses [Tokio] as its async runtime. The server would only need
to have a single endpoint, perhaps `/compile`.

#### Related possibilities

* Run an instance of the server on a platform like Heroku
* A `/functions` endpoint that describes the functions available in the current version
* The ability to share URLs for specific events and/or programs. This would require either making
  the server stateful and using some kind of persistent data store or devising a system of inferring
  events/programs via hashes passed in via URL. It's likely that those URLs would grow quite
  unwieldy, however, so we should experiment before taking this route.

### Published module

Yet another possibility worth considering would be publishing Wasm modules to hubs like [npm] and
[Wasmer] that others can use in their own environments. Those modules could be released alongside
versions of VRL. Tools from the Rust + Wasm ecosystem, like [wasm-pack], make this quite simple in
principle. With published modules, users could run VRL

It's unlikely, however, that this would be of any benefit to Vector users in the near term.

## Prior art

When assessing the overall benefit of expanding VRL into the web domain, I believe that the [Open
Policy Agent][opa] (OPA) project is a good place to look. OPA has a DSL called [Rego] that you use
to create policy logic. The maintainers of OPA created an [OPA Playground][opa_playground] that
enables you to experiment with the language using your own custom JSON inputs and policies, a
dramatically less cumbersome experience than using OPA on the command line (although, like VRL, OPA
has an interactive REPL). In that playground, you can "publish" your policies and share the
resulting URL. One of the core maintainers of OPA confirmed to me that those policies are stored in
Amazon S3.

## Plan of attack

TBD. I'd like to get feedback on these preliminary suggestions first and then circle back.

[6184]: https://github.com/timberio/vector/pull/6184
[6590]: https://github.com/timberio/vector/pull/6590
[npm]: https://npmjs.org
[opa]: https://open-policy-agent.org
[opa_playground]: https://play.openpolicyagent.org
[published wasm module]: #published-module
[react]: https://reactjs.org
[rego]: https://www.openpolicyagent.org/docs/latest/#rego
[tokio]: https://tokio.rs
[trunk]: https://github.com/thedodd/trunk
[vrl]: https://vrl.dev
[vrl_web_app]: https://vrl-web.netlify.app
[warp]: https://github.com/seanmonstar/warp
[wasm]: https://webassembly.org
[wasm-pack]: https://github.com/rustwasm/wasm-pack
[wasmer]: https://wasmer.io
[web app]: #interactive-web-app
[web server]: #web-server
[yew]: https://yew.rs
