---
title: Sources
sidebar_label: hidden
hide_pagination: true
---

Sources are responsible for ingesting [events][docs.data-model#event] into
Vector, they can both receive and pull in data. If you're deploying Vector in
an [agent role][docs.roles.agent], you'll want to user local data sources
like the [`file`][docs.sources.file] or [`stdin`][docs.sources.stdin] sources.
If you're deploying Vector in a [service role][docs.roles.service], you'll want
to use sources that receive data over the network, like the
[`vector`][docs.sources.vector], [`tcp`][docs.sources.tcp], and
[`syslog`][docs.sources.syslog] sources.

---

import VectorComponents from '@site/src/components/VectorComponents';

<VectorComponents titles={false} sinks={false} transforms={false} />


[docs.data-model#event]: /docs/about/data-model#event
[docs.roles.agent]: /docs/setup/deployment/roles/agent
[docs.roles.service]: /docs/setup/deployment/roles/service
[docs.sources.file]: /docs/reference/sources/file
[docs.sources.stdin]: /docs/reference/sources/stdin
[docs.sources.syslog]: /docs/reference/sources/syslog
[docs.sources.tcp]: /docs/reference/sources/tcp
[docs.sources.vector]: /docs/reference/sources/vector
