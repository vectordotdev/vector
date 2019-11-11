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

import Components from '@site/src/components/Components';

import Component from '@site/src/components/Component';

<Components titles={false}>

<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"docker_source"}
  name={"docker"}
  path="/docs/components/sources/docker"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"file_source"}
  name={"file"}
  path="/docs/components/sources/file"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"journald_source"}
  name={"journald"}
  path="/docs/components/sources/journald"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"kafka_source"}
  name={"kafka"}
  path="/docs/components/sources/kafka"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["metric"]}
  id={"statsd_source"}
  name={"statsd"}
  path="/docs/components/sources/statsd"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"stdin_source"}
  name={"stdin"}
  path="/docs/components/sources/stdin"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"syslog_source"}
  name={"syslog"}
  path="/docs/components/sources/syslog"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"tcp_source"}
  name={"tcp"}
  path="/docs/components/sources/tcp"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"udp_source"}
  name={"udp"}
  path="/docs/components/sources/udp"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log","metric"]}
  id={"vector_source"}
  name={"vector"}
  path="/docs/components/sources/vector"
  status={"beta"}
  type={"source"} />

</Components>

import Jump from '@site/src/components/Jump';

<Jump to="https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature" icon="plus-circle">
  Request a new source
</Jump>


[docs.data-model#event]: /docs/about/data-model#event
[docs.roles.agent]: /docs/setup/deployment/roles/agent
[docs.roles.service]: /docs/setup/deployment/roles/service
[docs.sources.file]: /docs/components/sources/file
[docs.sources.stdin]: /docs/components/sources/stdin
[docs.sources.syslog]: /docs/components/sources/syslog
[docs.sources.tcp]: /docs/components/sources/tcp
[docs.sources.vector]: /docs/components/sources/vector
