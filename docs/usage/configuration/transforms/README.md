---
description: Parse, structure, and transform events
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/README.md.erb
-->

# Transforms

![][images.transforms]

Transforms are in the middle of the [pipeline][docs.pipelines], sitting
in-between [sources][docs.sources] and [sinks][docs.sinks]. They transform
[events][docs.event] or the stream as a whole.

| Name  | Description |
|:------|:------------|
| [**`add_fields`**][docs.add_fields_transform] | Accepts [`log`][docs.log_event] events and allows you to add one or more fields. |
| [**`coercer`**][docs.coercer_transform] | Accepts [`log`][docs.log_event] events and allows you to coerce event fields into fixed types. |
| [**`field_filter`**][docs.field_filter_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to filter events by a field's value. |
| [**`grok_parser`**][docs.grok_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value with [Grok][url.grok]. |
| [**`json_parser`**][docs.json_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value as JSON. |
| [**`log_to_metric`**][docs.log_to_metric_transform] | Accepts [`log`][docs.log_event] events and allows you to convert logs into one or more metrics. |
| [**`lua`**][docs.lua_transform] | Accepts [`log`][docs.log_event] events and allows you to transform events with a full embedded [Lua][url.lua] engine. |
| [**`regex_parser`**][docs.regex_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field's value with a [Regular Expression][url.regex]. |
| [**`remove_fields`**][docs.remove_fields_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to remove one or more event fields. |
| [**`sampler`**][docs.sampler_transform] | Accepts [`log`][docs.log_event] events and allows you to sample events with a configurable rate. |
| [**`tokenizer`**][docs.tokenizer_transform] | Accepts [`log`][docs.log_event] events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zipping the tokens into ordered field names. |

[+ request a new transform][url.new_transform]


[docs.add_fields_transform]: ../../../usage/configuration/transforms/add_fields.md
[docs.coercer_transform]: ../../../usage/configuration/transforms/coercer.md
[docs.event]: ../../../about/data-model.md#event
[docs.field_filter_transform]: ../../../usage/configuration/transforms/field_filter.md
[docs.grok_parser_transform]: ../../../usage/configuration/transforms/grok_parser.md
[docs.json_parser_transform]: ../../../usage/configuration/transforms/json_parser.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.log_to_metric_transform]: ../../../usage/configuration/transforms/log_to_metric.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.metric_event]: ../../../about/data-model.md#metric
[docs.pipelines]: ../../../usage/configuration/README.md#composition
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.remove_fields_transform]: ../../../usage/configuration/transforms/remove_fields.md
[docs.sampler_transform]: ../../../usage/configuration/transforms/sampler.md
[docs.sinks]: ../../../usage/configuration/sinks
[docs.sources]: ../../../usage/configuration/sources
[docs.tokenizer_transform]: ../../../usage/configuration/transforms/tokenizer.md
[images.transforms]: ../../../assets/transforms.svg
[url.grok]: http://grokdebug.herokuapp.com/
[url.lua]: https://www.lua.org/
[url.new_transform]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.regex]: https://en.wikipedia.org/wiki/Regular_expression
