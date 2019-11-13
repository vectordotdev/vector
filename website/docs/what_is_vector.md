---
title: "What is Vector?"
---

import SVG from 'react-inlinesvg';

<SVG src="/img/components.svg" />

Vector is a [high-performance][docs.performance], [open-source][urls.vector_repo]
observability data router. It makes [collecting][docs.sources],
[transforming][docs.transforms], and [sending][docs.sinks] logs, metrics, &
events easy. It decouples data collection from your services, giving you data
ownership and control, among [many other benefits][docs.use_cases].

Built in [Rust][urls.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making it the single tool you need to get data
from A to B.


[docs.administration]: /docs/administration
[docs.correctness]: /docs/about/correctness
[docs.deployment]: /docs/setup/deployment
[docs.performance]: /docs/about/performance
[docs.roles.agent]: /docs/setup/deployment/roles/agent
[docs.roles.service]: /docs/setup/deployment/roles/service
[docs.sinks]: /docs/components/sinks
[docs.sources]: /docs/components/sources
[docs.transforms]: /docs/components/transforms
[docs.use_cases]: /docs/use_cases
[urls.rust]: https://www.rust-lang.org/
[urls.vector_repo]: https://github.com/timberio/vector
