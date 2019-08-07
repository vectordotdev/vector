---
description: Accepts `log` and `metric` events and allows you to filter events by a field's value.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/field_filter.md.erb
-->

# field_filter transform

![][images.field_filter_transform]

{% hint style="warning" %}
The `field_filter` transform is in beta. Please see the current
[enhancements][url.field_filter_transform_enhancements] and
[bugs][url.field_filter_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_field_filter_transform_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `field_filter` transform accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to filter events by a field's value.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  type = "field_filter" # must be: "field_filter"
  inputs = ["my-source-id"]
  field = "file"
  value = "/var/log/nginx.log"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  type = "field_filter"
  inputs = ["<string>", ...]
  field = "<string>"
  value = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.field_filter_transform]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "field_filter"
  type = "field_filter"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The target field to compare against the `value`.
  # 
  # * required
  # * no default
  field = "file"

  # If the value of the specified `field` matches this value then the event will
  # be permitted, otherwise it is dropped.
  # 
  # * required
  # * no default
  value = "/var/log/nginx.log"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "field_filter"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `field` | `string` | The target field to compare against the `value`.<br />`required` `example: "file"` |
| `value` | `string` | If the value of the specified `field` matches this value then the event will be permitted, otherwise it is dropped.<br />`required` `example: "/var/log/nginx.log"` |

## How It Works

### Complex Comparisons

The `field_filter` transform is designed for simple equality filtering, it is
not designed for complex comparisons. There are plans to build a `filter`
transform that accepts more complex filtering.

We've opened [issue 479][url.issue_479] for more complex filtering.

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

1. Check for any [open `field_filter_transform` issues][url.field_filter_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_field_filter_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_field_filter_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.lua_transform]

## Resources

* [**Issues**][url.field_filter_transform_issues] - [enhancements][url.field_filter_transform_enhancements] - [bugs][url.field_filter_transform_bugs]
* [**Source code**][url.field_filter_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.field_filter_transform]: ../../../assets/field_filter-transform.svg
[url.field_filter_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+field_filter%22+label%3A%22Type%3A+Bug%22
[url.field_filter_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+field_filter%22+label%3A%22Type%3A+Enhancement%22
[url.field_filter_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+field_filter%22
[url.field_filter_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/field_filter.rs
[url.issue_479]: https://github.com/timberio/vector/issues/479
[url.new_field_filter_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+field_filter&labels=Type%3A+Bug
[url.new_field_filter_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+field_filter&labels=Type%3A+Enhancement
[url.new_field_filter_transform_issue]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+field_filter
[url.vector_chat]: https://chat.vector.dev
