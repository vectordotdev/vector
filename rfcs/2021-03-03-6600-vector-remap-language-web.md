# RFC 6600 - 2021-03-03 - Vector Remap Language on the web

## Interactive web app

## Web server

This would entail creating a simple HTTP server with one endpoint to which you
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

A server of this kind would be germane to "serverless" environments like Lambda,
as the logic is fully stateless.



* A basic HTTP server to which you can `POST` VRL programs and events and get an event/error back. A related option would entail allowing you to pass compiler state back and forth with the server as well, for use in interactive environments.
* A full-fledged interactive web app (like the nascent one in #6590) for exploring VRL. That app could be backed by either (a) the HTTP server mentioned above or (b) a Wasm binary or (c) both.
* A published standalone
