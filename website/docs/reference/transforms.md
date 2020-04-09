---
title: Transforms
description: "Transforms are responsible for parsing, enriching, or transforming your log and metric data in-flight. They can be chained together, forming a network of transforms within your topology."
sidebar_label: hidden
hide_pagination: true
---

import VectorComponents from '@site/src/components/VectorComponents';

Transforms are responsible for parsing, enriching, or transforming your
[log][docs.data-model.log] and [metric][docs.data-model.metric] data
in-flight. They can be chained together, forming a network of transforms within
your topology, ultimately flowing into a [sink][docs.sinks].

---

<VectorComponents titles={false} sinks={false} sources={false} />


[docs.data-model.log]: /docs/about/data-model/log/
[docs.data-model.metric]: /docs/about/data-model/metric/
[docs.sinks]: /docs/reference/sinks/
