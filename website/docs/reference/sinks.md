---
title: Sinks
description: "Vector sinks fan-out logs and metrics data to a variety of downstream destinations. These could be exact services, like Elasticsearch, or generic protocols, like HTTP or TCP."
sidebar_label: hidden
hide_pagination: true
---

Vector sinks fan-out [log][docs.data-model.log] and
[metric][docs.data-model.metric] [events][docs.data-model#event] data to a
variety of downstream destinations. They are responsible for reliably sending,
or outputting, this data.

---

import VectorComponents from '@site/src/components/VectorComponents';

<VectorComponents titles={false} sources={false} transforms={false} />


[docs.data-model#event]: /docs/about/data-model/#event
[docs.data-model.log]: /docs/about/data-model/log/
[docs.data-model.metric]: /docs/about/data-model/metric/
