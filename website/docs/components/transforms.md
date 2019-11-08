---
title: Transforms
sidebar_label: hidden
hide_pagination: true
---

Transforms are in the middle of the [pipeline][docs.configuration#composition],
sitting in-between [sources][docs.sources] and [sinks][docs.sinks]. They
transform [events][docs.data-model#event] or the stream as a whole.

import Components from '@site/src/components/Components';

import Component from '@site/src/components/Component';

<Components>

<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"add_fields_transform"}
  name={"add_fields"}
  path="../components/transforms/add_fields"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["metric"]}
  id={"add_tags_transform"}
  name={"add_tags"}
  path="../components/transforms/add_tags"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"coercer_transform"}
  name={"coercer"}
  path="../components/transforms/coercer"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log","metric"]}
  id={"field_filter_transform"}
  name={"field_filter"}
  path="../components/transforms/field_filter"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"grok_parser_transform"}
  name={"grok_parser"}
  path="../components/transforms/grok_parser"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"json_parser_transform"}
  name={"json_parser"}
  path="../components/transforms/json_parser"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log","metric"]}
  id={"log_to_metric_transform"}
  name={"log_to_metric"}
  path="../components/transforms/log_to_metric"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"lua_transform"}
  name={"lua"}
  path="../components/transforms/lua"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"regex_parser_transform"}
  name={"regex_parser"}
  path="../components/transforms/regex_parser"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"remove_fields_transform"}
  name={"remove_fields"}
  path="../components/transforms/remove_fields"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["metric"]}
  id={"remove_tags_transform"}
  name={"remove_tags"}
  path="../components/transforms/remove_tags"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"sampler_transform"}
  name={"sampler"}
  path="../components/transforms/sampler"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"split_transform"}
  name={"split"}
  path="../components/transforms/split"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"tokenizer_transform"}
  name={"tokenizer"}
  path="../components/transforms/tokenizer"
  status={"prod-ready"}
  type={"transform"} />

</Components>

import Jump from '@site/src/components/Jump';

<Jump to="https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature" icon="plus-circle">
  Request a new transform
</Jump>


[docs.configuration#composition]: ../setup/configuration#composition
[docs.data-model#event]: ../about/data-model#event
[docs.sinks]: ../components/sinks
[docs.sources]: ../components/sources
