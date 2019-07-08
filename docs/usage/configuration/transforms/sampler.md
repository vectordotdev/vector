---
description: Accepts `log` events and allows you to sample events with a configurable rate.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/sampler.md.erb
-->

# sampler transform

![][images.sampler_transform]

{% hint style="warning" %}
The `sampler` sink is in beta. Please see the current
[enhancements][url.sampler_transform_enhancements] and
[bugs][url.sampler_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_sampler_transform_issues]
as it will help shape the roadmap of this component.
{% endhint %}

The `sampler` transform accepts [`log`][docs.log_event] events and allows you to sample events with a configurable rate.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sampler_transform_id]
  # REQUIRED
  type = "sampler" # must be: "sampler"
  inputs = ["my-source-id"]
  
  # OPTIONAL
  pass_list = ["[error]", "field2"] # no default
  rate = ["field1", "field2"] # no default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  type = "sampler"
  inputs = ["<string>", ...]
  pass_list = ["<string>", ...]
  rate = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.sampler]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "sampler"
  type = "sampler"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # A list of regular expression patterns to exclude events from sampling. If an
  # event's `"message"` key matches _any_ of these patterns it will _not_ be
  # sampled.
  # 
  # * optional
  # * no default
  pass_list = ["[error]", "field2"]

  # The maximum number of events allowed per second.
  # 
  # * optional
  # * no default
  rate = ["field1", "field2"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `enum: "sampler"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **OPTIONAL** | | |
| `pass_list` | `[string]` | A list of regular expression patterns to exclude events from sampling. If an event's `"message"` key matches _any_ of these patterns it will _not_ be sampled.<br />`no default` `example: ["[error]", "field2"]` |
| `rate` | `int` | The maximum number of events allowed per second.<br />`no default` `example: ["field1", "field2"]` |

## How It Works



## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.sampler_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.sampler_transform_issues] - [enhancements][url.sampler_transform_enhancements] - [bugs][url.sampler_transform_bugs]
* [**Source code**][url.sampler_transform_source]


[docs.config_composition]: https://docs.vector.dev/usage/configuration/README#composition
[docs.log_event]: https://docs.vector.dev/about/data-model#log
[docs.monitoring_logs]: https://docs.vector.dev/usage/administration/monitoring#logs
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.transforms]: https://docs.vector.dev/usage/configuration/transforms
[docs.troubleshooting]: https://docs.vector.dev/usage/guides/troubleshooting
[images.sampler_transform]: https://docs.vector.dev/assets/sampler-transform.svg
[url.community]: https://vector.dev/community
[url.new_sampler_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+new_sampler%22
[url.sampler_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22+label%3A%22Type%3A+Bug%22
[url.sampler_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22+label%3A%22Type%3A+Enhancement%22
[url.sampler_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22
[url.sampler_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/sampler.rs
[url.search_forum]: https://forum.vector.dev/search?expanded=true
