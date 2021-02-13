---
last_modified_on: "2020-02-15"
$schema: ".schema.json"
title: "Introducing `vector top`"
description: "A CLI dashboard interface for monitoring Vector instances."
author_github: "https://github.com/lucperkins"
featured: true
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
tags: ["type: featured", "domain: api"]
---

[`vector top`][top] is a command for the Vector [CLI] that displays both metrics emitted by your Vector instance as well
as information about your Vector [topology] through a beautiful dashboard-style interface reminiscent of tools like
[htop]. To use it, run `vector top` and specify the URL of the running Vector instance you want to monitor:

```bash
vector top --url https://my-vector-instance.prod.acmecorp.biz
```

By default, the `vector top` looks for a Vector instance running locally at http://localhost:8686, but you can also
monitor remote instances, as in the example above.

Architecturally, `vector top` interacts directly with Vector's [GraphQL API][api], which was built with `vector top` as
a primary consumer. The dashboard UI was created using the excellent [tui-rs] library.

[api]: https://vector.dev/docs/reference/api
[cli]: https://vector.dev/docs/reference/cli
[htop]: https://htop.dev
[top]: https://vector.dev/docs/reference/cli/#top
[topology]: https://vector.dev/docs/about/concepts/#topology
[tui]: https://docs.rs/tui
