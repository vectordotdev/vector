---
title: "What is Vector?"
---

import SVG from 'react-inlinesvg';

<SVG src="/img/components.svg" />

Vector is a [high-performance][pages.index#performance], [open-source][urls.vector_repo]
observability data router. It makes [collecting][docs.sources],
[transforming][docs.transforms], and [sending][docs.sinks] logs, metrics and
events easy. It decouples data collection from your services, giving you data
ownership and control, among many other benefits.

Built in [Rust][urls.rust], Vector places high-value on
[performance][pages.index#performance], [correctness][pages.index#correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making it the single tool you need to get data
from A to B.

import Jump from '@site/src/components/Jump';

<Jump to="/docs/setup/guides/getting-started/">Get started</Jump>


[docs.administration]: /docs/administration/
[docs.deployment]: /docs/setup/deployment/
[docs.roles.agent]: /docs/setup/deployment/roles/agent/
[docs.roles.service]: /docs/setup/deployment/roles/service/
[docs.sinks]: /docs/reference/sinks/
[docs.sources]: /docs/reference/sources/
[docs.transforms]: /docs/reference/transforms/
[pages.index#correctness]: /#correctness
[pages.index#performance]: /#performance
[urls.rust]: https://www.rust-lang.org/
[urls.vector_repo]: https://github.com/timberio/vector
