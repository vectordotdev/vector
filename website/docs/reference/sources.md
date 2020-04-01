---
last_modified_on: "2020-03-31"
title: Sources
description: "Sources are reponsible for collect or receiving log and metrics data. These could be local sources, like a file, or a protocols, like HTTP or TCP."
sidebar_label: hidden
hide_pagination: true
---

import VectorComponents from '@site/src/components/VectorComponents';

Sources are responsible for ingesting [events][docs.data-model] into
Vector, they can both receive and pull in data. If you're deploying Vector as
a [daemon][docs.strategies#daemon] or [sidecar][docs.strategies#sidecar], you'll
want to user local data sources like the [`file`][docs.sources.file] or
[`stdin`][docs.sources.stdin] sources. If you're deploying Vector as a
[service][docs.strategies#service], you'll want to use sources that receive data
over the network, like the [`vector`][docs.sources.vector],
[`socket`][docs.sources.socket], and [`syslog`][docs.sources.syslog] sources.

---

<VectorComponents titles={false} sinks={false} transforms={false} />


[docs.data-model]: /docs/about/data-model/
[docs.sources.file]: /docs/reference/sources/file/
[docs.sources.socket]: /docs/reference/sources/socket/
[docs.sources.stdin]: /docs/reference/sources/stdin/
[docs.sources.syslog]: /docs/reference/sources/syslog/
[docs.sources.vector]: /docs/reference/sources/vector/
[docs.strategies#daemon]: /docs/setup/deployment/strategies/#daemon
[docs.strategies#service]: /docs/setup/deployment/strategies/#service
[docs.strategies#sidecar]: /docs/setup/deployment/strategies/#sidecar
