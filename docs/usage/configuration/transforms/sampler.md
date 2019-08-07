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
The `sampler` transform is in beta. Please see the current
[enhancements][url.sampler_transform_enhancements] and
[bugs][url.sampler_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_sampler_transform_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `sampler` transform accepts [`log`][docs.log_event] events and allows you to sample events with a configurable rate.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  type = "sampler" # must be: "sampler"
  inputs = ["my-source-id"]
  rate = 10
  
  pass_list = ["[error]", "field2"] # no default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  type = "sampler"
  inputs = ["<string>", ...]
  rate = <int>
  pass_list = ["<string>", ...]
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.sampler_transform]
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

  # The maximum number of events allowed per second.
  # 
  # * required
  # * no default
  rate = 10

  # A list of regular expression patterns to exclude events from sampling. If an
  # event's `"message"` key matches _any_ of these patterns it will _not_ be
  # sampled.
  # 
  # * optional
  # * no default
  pass_list = ["[error]", "field2"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "sampler"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `rate` | `int` | The maximum number of events allowed per second.<br />`required` `example: 10` |
| **OPTIONAL** | | |
| `pass_list` | `[string]` | A list of regular expression patterns to exclude events from sampling. If an event's `"message"` key matches _any_ of these patterns it will _not_ be sampled.<br />`no default` `example: ["[error]", "field2"]` |

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `sampler_transform` issues][url.sampler_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_sampler_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_sampler_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.lua_transform]

## Resources

* [**Issues**][url.sampler_transform_issues] - [enhancements][url.sampler_transform_enhancements] - [bugs][url.sampler_transform_bugs]
* [**Source code**][url.sampler_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.sampler_transform]: ../../../assets/sampler-transform.svg
[url.new_sampler_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+sampler&labels=Type%3A+Bug
[url.new_sampler_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+sampler&labels=Type%3A+Enhancement
[url.new_sampler_transform_issue]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+sampler
[url.sampler_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22+label%3A%22Type%3A+Bug%22
[url.sampler_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22+label%3A%22Type%3A+Enhancement%22
[url.sampler_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+sampler%22
[url.sampler_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/sampler.rs
[url.vector_chat]: https://chat.vector.dev
