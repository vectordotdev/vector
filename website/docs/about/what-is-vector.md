---
title: "What is Vector?"
description: "High-level description of the Vector observability data collector and router."
---

import SVG from 'react-inlinesvg';

<SVG src="/img/components.svg" />

Vector is a _highly reliable_ observability data router built for demanding
production environments. Vector is designed on the following principles:

* **High reliability** - Built in [Rust][urls.rust], Vector is [memory safe][urls.rust_memory_safety], [correct][pages.index#correctness], and [performant][pages.index#performance].
* **Operator safety** - Vector is pragmatic and hard to break. It avoids the common pitfalls in similar tools.
* **All data** - [Logs][docs.data-model.log], [metrics][docs.data-model.metric], and traces (coming soon). A [sophisticated data model][docs.data-model] enables _correct_ interoperability.
* **One tool** - Deploys as an [agent][docs.roles.agent] or [service][docs.roles.service]. One tool gets your data from A to B.

Vector is **deployed over 100,000 times per day**, and is trusted by Fortune 500
comapanies and forward thinking engineering teams.

import Jump from '@site/src/components/Jump';

<Jump to="/docs/setup/guides/getting-started/">Get started</Jump>


[docs.data-model.log]: /docs/about/data-model/log/
[docs.data-model.metric]: /docs/about/data-model/metric/
[docs.data-model]: /docs/about/data-model/
[docs.roles.agent]: /docs/setup/deployment/roles/agent/
[docs.roles.service]: /docs/setup/deployment/roles/service/
[pages.index#correctness]: /#correctness
[pages.index#performance]: /#performance
[urls.rust]: https://www.rust-lang.org/
[urls.rust_memory_safety]: https://hacks.mozilla.org/2019/01/fearless-security-memory-safety/
