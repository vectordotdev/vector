---
date: "2021-04-21"
title: "Introducing `vector tap`"
description: "Observing events flowing through your Vector topologies"
authors: ["jszwedko"]
pr_numbers: [6871]
release: "0.13.0"
hide_on_release_notes: false
badges:
  type: new feature
  domains: ["graphql", "cli", "observability"]
---

Vector 0.13 introduces a new [`vector tap`][tap] subcommand that allows for
tapping into the events flowing through Vector. This can be used to "live tail"
events while troubleshooting incidents events or simply to debug your Vector
config, replacing the current common approach of sending the events to
a [`console`][console] sink.

## Get Started

To use [`vector tap`][tap] you must first enable Vector's [GraphQL API][api].
Then, you can use `vector tap <component id>` to sample the events flowing out
of this component.

For example, given the configuration:

```toml
[api]
  enabled =  true
[sources.in]
  type = "generator"
  format = "shuffle"
  interval = 1.0
  lines = ["Hello World"]
  sequence = true

[sinks.out]
  type = "blackhole"
  inputs = ["in"]
```

If you were to run `vector` and then, in another terminal, run `vector tap in`,
you would see something like:

```json
{"message":"13 Hello World","timestamp":"2021-04-20T19:40:32.359390Z"}
{"message":"14 Hello World","timestamp":"2021-04-20T19:40:33.355298Z"}
{"message":"15 Hello World","timestamp":"2021-04-20T19:40:34.353215Z"}
{"message":"16 Hello World","timestamp":"2021-04-20T19:40:35.353493Z"}
{"message":"17 Hello World","timestamp":"2021-04-20T19:40:36.352089Z"}
{"message":"18 Hello World","timestamp":"2021-04-20T19:40:37.347406Z"}
```

With the events formatted as JSON.

## API

Like [`vector top`][top], this command is made possible by `vector` through its
[GraphQL API][api]. You can interact directly with the API if you want to take
advantage of tapping events programmatically.

## Future Work

We intend to make the [`vector tap`][tap] command even more powerful by:

- Allowing sampling of metrics events
- Allowing events to be formatted as logfmt rather than just JSON or YAML
- Allowing sampling of events going _into_ a component rather than out of it
- Allowing tighter control over how events are sampled

Have an idea of how to make [`vector tap`][tap] even more useful? [Let us
know][community].

[api]: /docs/reference/api/
[community]: /community/
[console]: /docs/reference/configuration/sinks/console/
[top]: /docs/reference/cli/#top
[tap]: /docs/reference/cli/#tap
